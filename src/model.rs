use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// 请求参数模型：可序列化为微信要求的 JSON。
pub trait ParamsTrait: Serialize {
    /// 序列化为微信要求的 JSON 字符串。
    ///
    /// 默认实现挂在 trait 上：原先每个请求参数类型都复制一份
    /// `serde_json::to_string(self).unwrap()`，其中 `unwrap` 是库路径上的 panic，
    /// 而现在失败会作为 [`PayError`](crate::error::PayError) 返回。
    fn to_json(&self) -> Result<String, crate::error::PayError> {
        Ok(serde_json::to_string(self)?)
    }
}

/// 货币类型：仅支持人民币 CNY。
#[derive(Serialize, Debug, Clone)]
pub enum Currency {
    /// 人民币。
    CNY,
}

impl Display for Currency {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Currency::CNY => write!(f, "CNY"),
        }
    }
}

/// 订单金额信息。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AmountInfo {
    ///【标价金额】 订单总金额，单位为分。
    pub total: i32,
}

impl From<i32> for AmountInfo {
    fn from(value: i32) -> Self {
        Self { total: value }
    }
}

/// 支付者信息。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PayerInfo {
    ///【用户标识】 用户在直连商户appid下的唯一标识。
    pub openid: String,
}

impl From<&str> for PayerInfo {
    fn from(value: &str) -> Self {
        Self {
            openid: value.to_string(),
        }
    }
}

/// 单品（商品）信息。
#[derive(Serialize, Debug, Clone)]
pub struct GoodsDetail {
    ///【商户侧商品编码】 由半角的大小写字母、数字、中划线、下划线中的一种或几种组成。
    pub merchant_goods_id: String,
    ///【商品数量】 用户购买的数量
    pub quantity: i32,
    ///【商品单价】 单位为：分。如果商户有优惠，需传输商户优惠后的单价(例如：用户对一笔100元的订单使用了商场发的纸质优惠券100-50，则活动商品的单价应为原单价-50)
    pub unit_price: i32,
    ///【微信支付商品编码】 微信支付定义的统一商品编号（没有可不传）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wechatpay_goods_id: Option<String>,
    ///【商品名称】 商品的实际名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goods_name: Option<String>,
}

/// `serde` 跳过条件：`None` 与空数组都不写入 JSON。
///
/// 用于 `goods_detail` 这类「要么不传，要么至少一条」的字段 —— 显式传 `Some(vec![])`
/// 与传 `None` 必须得到同一个请求体，否则调用方会发出官方明文禁止的空数组。
fn is_none_or_empty<T>(value: &Option<Vec<T>>) -> bool {
    value.as_ref().is_none_or(Vec::is_empty)
}

/// 订单优惠信息。
#[derive(Serialize, Debug, Clone)]
pub struct OrderDetail {
    ///【订单原价】
    /// 1、商户侧一张小票订单可能被分多次支付，订单原价用于记录整张小票的交易金额。
    /// 2、当订单原价与支付金额不相等，则不享受优惠。
    /// 3、该字段主要用于防止同一张小票分多次支付，以享受多次优惠的情况，正常支付订单不必上传此参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_price: Option<i32>,
    ///【商品小票ID】 商家小票ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_id: Option<String>,
    ///【单品列表】 单品列表信息，条目个数限制：【1，6000】。不传（或传空数组）时整个字段
    /// 都不出现 —— 空数组不在官方允许的取值范围内，`None` 与 `Some(vec![])` 语义相同。
    #[serde(skip_serializing_if = "is_none_or_empty")]
    pub goods_detail: Option<Vec<GoodsDetail>>,
}

/// 商户门店信息。
#[derive(Serialize, Debug, Clone)]
pub struct StoreInfo {
    ///【门店编号】 商户侧门店编号
    pub id: String,
    ///【门店名称】 商户侧门店名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    ///【地区编码】 地区编码，详细请见省市区编号对照表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_code: Option<String>,
    ///【详细地址】 详细的商户门店地址
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

/// 支付场景信息。
#[derive(Serialize, Debug, Clone)]
pub struct SceneInfo {
    ///【用户终端IP】 用户的客户端IP，支持IPv4和IPv6两种格式的IP地址。
    pub payer_client_ip: String,
    ///【商户端设备号】 商户端设备号（门店号或收银设备ID）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    ///【商户门店信息】 商户门店信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_info: Option<StoreInfo>,
}

/// H5 场景类型：iOS / Android / WAP。
///
/// 变体的 `Serialize` 与 `Display` 现在给出同一个字符串（官方取值里 `iOS` 的 `S` 是大写）——
/// 在这之前同一个变体序列化成 `Ios`，而 `Display` 输出 `iOS`。
/// ⚠ [`H5Info::h5_type`] 目前是 `String`，请求体并不直接由本枚举序列化。
#[derive(Serialize, Debug, Clone)]
pub enum H5Type {
    /// iOS
    #[serde(rename = "iOS")]
    Ios,
    /// Android
    #[serde(rename = "Android")]
    Android,
    /// WAP
    #[serde(rename = "Wap")]
    Wap,
}

impl Display for H5Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            H5Type::Ios => write!(f, "iOS"),
            H5Type::Android => write!(f, "Android"),
            H5Type::Wap => write!(f, "Wap"),
        }
    }
}

/// H5 场景信息。
#[derive(Serialize, Debug, Clone)]
pub struct H5Info {
    ///【场景类型】 场景类型
    #[serde(rename = "type")]
    pub h5_type: String,
    ///【应用名称】 应用名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    ///【网站URL】 网站URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_url: Option<String>,
    ///【iOS平台BundleID】 iOS平台BundleID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    ///【Android平台PackageName】 Android平台PackageName
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
}

/// H5 支付场景信息。
#[derive(Serialize, Debug, Clone)]
pub struct H5SceneInfo {
    ///【用户终端IP】 用户的客户端IP，支持IPv4和IPv6两种格式的IP地址。
    pub payer_client_ip: String,
    ///【H5场景信息】
    pub h5_info: H5Info,
    ///【商户端设备号】 商户端设备号（门店号或收银设备ID）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    ///【商户门店信息】 商户门店信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_info: Option<StoreInfo>,
}

impl H5SceneInfo {
    /// 构造 H5SceneInfo。
    pub fn new<S: AsRef<str>>(payer_client_ip: S, app_name: S, app_url: S) -> Self {
        Self {
            payer_client_ip: payer_client_ip.as_ref().to_string(),
            h5_info: H5Info {
                h5_type: H5Type::Wap.to_string(),
                app_name: Some(app_name.as_ref().to_string()),
                app_url: Some(app_url.as_ref().to_string()),
                bundle_id: None,
                package_name: None,
            },
            device_id: None,
            store_info: None,
        }
    }
}

impl ParamsTrait for SceneInfo {}

/// JSAPI 下单参数：小程序支付。
#[derive(Serialize, Debug, Clone)]
pub struct JsapiParams {
    ///【商品描述】 商品描述
    pub description: String,
    ///【商户订单号】 商户系统内部订单号，只能是数字、大小写字母_-*且在同一个商户号下唯一。
    pub out_trade_no: String,
    ///【订单金额】 订单金额信息
    pub amount: AmountInfo,
    ///【支付者】 支付者信息
    pub payer: PayerInfo,
    ///【附加数据】 附加数据，在查询API和支付通知中原样返回，可作为自定义参数使用，实际情况下只有支付完成状态才会返回该字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach: Option<String>,
    ///【优惠功能】 优惠功能
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<OrderDetail>,
    ///【交易结束时间】 订单失效时间，遵循rfc3339标准格式，格式为yyyy-MM-DDTHH:mm:ss+TIMEZONE，yyyy-MM-DD表示年月日，T出现在字符串中，表示time元素的开头，HH:mm:ss表示时分秒，TIMEZONE表示时区（+08:00表示东八区时间，领先UTC8小时，即北京时间）。例如：2015-05-20T13:29:35+08:00表示，北京时间2015年5月20日13点29分35秒。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_expire: Option<String>,
    ///【场景信息】 支付场景描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_info: Option<SceneInfo>,
}

impl ParamsTrait for JsapiParams {}

/// 付款码支付参数：商户扫描用户付款码收款。
#[derive(Serialize, Debug, Clone)]
pub struct MicroParams {
    ///【商品描述】 商品描述
    pub description: String,
    ///【商户订单号】 商户系统内部订单号，只能是数字、大小写字母_-*且在同一个商户号下唯一。
    pub out_trade_no: String,
    ///【订单金额】 订单金额信息
    pub amount: AmountInfo,
    ///【支付者】 支付者信息
    pub payer: PayerInfo,
    ///【附加数据】 附加数据，在查询API和支付通知中原样返回，可作为自定义参数使用，实际情况下只有支付完成状态才会返回该字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach: Option<String>,
    ///【优惠功能】 优惠功能
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<OrderDetail>,
    ///【交易结束时间】 订单失效时间，遵循rfc3339标准格式，格式为yyyy-MM-DDTHH:mm:ss+TIMEZONE，yyyy-MM-DD表示年月日，T出现在字符串中，表示time元素的开头，HH:mm:ss表示时分秒，TIMEZONE表示时区（+08:00表示东八区时间，领先UTC8小时，即北京时间）。例如：2015-05-20T13:29:35+08:00表示，北京时间2015年5月20日13点29分35秒。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_expire: Option<String>,
    ///【场景信息】 支付场景描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_info: Option<SceneInfo>,
}

impl ParamsTrait for MicroParams {}

impl MicroParams {
    /// 构造 MicroParams。
    pub fn new<S: AsRef<str>>(
        description: S,
        out_trade_no: S,
        amount: AmountInfo,
        payer: PayerInfo,
    ) -> Self {
        Self {
            description: description.as_ref().to_string(),
            out_trade_no: out_trade_no.as_ref().to_string(),
            amount,
            payer,
            time_expire: None,
            attach: None,
            detail: None,
            scene_info: None,
        }
    }
}

impl JsapiParams {
    /// 构造 JsapiParams。
    pub fn new<S: AsRef<str>>(
        description: S,
        out_trade_no: S,
        amount: AmountInfo,
        payer: PayerInfo,
    ) -> Self {
        Self {
            description: description.as_ref().to_string(),
            out_trade_no: out_trade_no.as_ref().to_string(),
            amount,
            payer,
            time_expire: None,
            attach: None,
            detail: None,
            scene_info: None,
        }
    }
}

/// 结算信息。
#[derive(Serialize, Debug, Clone)]
pub struct SettleInfo {
    ///【是否指定分账】 是否指定分账，
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profit_sharing: Option<bool>,
}

/// Native 下单参数：扫码支付。
#[derive(Serialize, Debug, Clone)]
pub struct NativeParams {
    ///【商品描述】 商品描述
    pub description: String,
    ///【通知地址】 异步接收微信支付结果通知的回调地址，通知URL必须为外网可访问的URL，不能携带参数。 公网域名必须为HTTPS，如果是走专线接入，使用专线NAT IP或者私有回调域名可使用HTTP
    /// pub notify_url: String,
    ///【商户订单号】 商户系统内部订单号，只能是数字、大小写字母_-*且在同一个商户号下唯一。
    pub out_trade_no: String,
    ///【订单金额】 订单金额信息
    pub amount: AmountInfo,
    ///【交易结束时间】 订单失效时间，遵循rfc3339标准格式，格式为yyyy-MM-DDTHH:mm:ss+TIMEZONE，yyyy-MM-DD表示年月日，T出现在字符串中，表示time元素的开头，HH:mm:ss表示时分秒，TIMEZONE表示时区（+08:00表示东八区时间，领先UTC8小时，即北京时间）。例如：2015-05-20T13:29:35+08:00表示，北京时间2015年5月20日13点29分35秒。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_expire: Option<String>,
    ///【附加数据】 附加数据，在查询API和支付通知中原样返回，可作为自定义参数使用，实际情况下只有支付完成状态才会返回该字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach: Option<String>,
    ///【订单优惠标记】 商品标记，代金券或立减优惠功能的参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goods_tag: Option<String>,
    ///【电子发票入口开放标识】 传入true时，支付成功消息和支付详情页将出现开票入口。需要在微信支付商户平台或微信公众平台开通电子发票功能，传此字段才可生效。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_fapiao: Option<bool>,
    ///【场景信息】 支付场景描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_info: Option<SceneInfo>,
    ///【结算信息】 结算信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settle_info: Option<SettleInfo>,
}

impl ParamsTrait for NativeParams {}

/// APP 下单参数：APP 支付。
#[derive(Serialize, Debug, Clone)]
pub struct AppParams {
    ///【商品描述】 商品描述
    pub description: String,
    ///【通知地址】 异步接收微信支付结果通知的回调地址，通知URL必须为外网可访问的URL，不能携带参数。 公网域名必须为HTTPS，如果是走专线接入，使用专线NAT IP或者私有回调域名可使用HTTP
    /// pub notify_url: String,
    ///【商户订单号】 商户系统内部订单号，只能是数字、大小写字母_-*且在同一个商户号下唯一。
    pub out_trade_no: String,
    ///【订单金额】 订单金额信息
    pub amount: AmountInfo,
    ///【交易结束时间】 订单失效时间，遵循rfc3339标准格式，格式为yyyy-MM-DDTHH:mm:ss+TIMEZONE，yyyy-MM-DD表示年月日，T出现在字符串中，表示time元素的开头，HH:mm:ss表示时分秒，TIMEZONE表示时区（+08:00表示东八区时间，领先UTC8小时，即北京时间）。例如：2015-05-20T13:29:35+08:00表示，北京时间2015年5月20日13点29分35秒。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_expire: Option<String>,
    ///【附加数据】 附加数据，在查询API和支付通知中原样返回，可作为自定义参数使用，实际情况下只有支付完成状态才会返回该字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach: Option<String>,
    ///【订单优惠标记】 商品标记，代金券或立减优惠功能的参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goods_tag: Option<String>,
    ///【电子发票入口开放标识】 传入true时，支付成功消息和支付详情页将出现开票入口。需要在微信支付商户平台或微信公众平台开通电子发票功能，传此字段才可生效。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_fapiao: Option<bool>,
    ///【优惠功能】 优惠功能
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<OrderDetail>,
    ///【场景信息】 支付场景描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_info: Option<SceneInfo>,
    ///【结算信息】 结算信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settle_info: Option<SettleInfo>,
}

impl ParamsTrait for AppParams {}

impl AppParams {
    /// 构造 AppParams。
    pub fn new<S: AsRef<str>>(description: S, out_trade_no: S, amount: AmountInfo) -> Self {
        Self {
            description: description.as_ref().to_string(),
            out_trade_no: out_trade_no.as_ref().to_string(),
            amount,
            time_expire: None,
            attach: None,
            goods_tag: None,
            support_fapiao: None,
            detail: None,
            scene_info: None,
            settle_info: None,
        }
    }
}

/// H5 下单参数：H5 网页支付。
#[derive(Serialize, Debug, Clone)]
pub struct H5Params {
    ///【商品描述】 商品描述
    pub description: String,
    ///【通知地址】 异步接收微信支付结果通知的回调地址，通知URL必须为外网可访问的URL，不能携带参数。 公网域名必须为HTTPS，如果是走专线接入，使用专线NAT IP或者私有回调域名可使用HTTP
    /// pub notify_url: String,
    ///【商户订单号】 商户系统内部订单号，只能是数字、大小写字母_-*且在同一个商户号下唯一。
    pub out_trade_no: String,
    ///【订单金额】 订单金额信息
    pub amount: AmountInfo,
    ///【交易结束时间】 订单失效时间，遵循rfc3339标准格式，格式为yyyy-MM-DDTHH:mm:ss+TIMEZONE，yyyy-MM-DD表示年月日，T出现在字符串中，表示time元素的开头，HH:mm:ss表示时分秒，TIMEZONE表示时区（+08:00表示东八区时间，领先UTC8小时，即北京时间）。例如：2015-05-20T13:29:35+08:00表示，北京时间2015年5月20日13点29分35秒。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_expire: Option<String>,
    ///【附加数据】 附加数据，在查询API和支付通知中原样返回，可作为自定义参数使用，实际情况下只有支付完成状态才会返回该字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach: Option<String>,
    ///【订单优惠标记】 商品标记，代金券或立减优惠功能的参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goods_tag: Option<String>,
    ///【电子发票入口开放标识】 传入true时，支付成功消息和支付详情页将出现开票入口。需要在微信支付商户平台或微信公众平台开通电子发票功能，传此字段才可生效。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_fapiao: Option<bool>,
    ///【场景信息】 支付场景描述
    pub scene_info: H5SceneInfo,
    ///【结算信息】 结算信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settle_info: Option<SettleInfo>,
}

impl ParamsTrait for H5Params {}

impl H5Params {
    /// 构造 H5Params。
    pub fn new<S: AsRef<str>>(
        description: S,
        out_trade_no: S,
        amount: AmountInfo,
        scene_info: H5SceneInfo,
    ) -> Self {
        Self {
            description: description.as_ref().to_string(),
            out_trade_no: out_trade_no.as_ref().to_string(),
            amount,
            time_expire: None,
            attach: None,
            goods_tag: None,
            support_fapiao: None,
            scene_info,
            settle_info: None,
        }
    }
}

impl NativeParams {
    /// 构造 NativeParams。
    pub fn new<S: AsRef<str>>(description: S, out_trade_no: S, amount: AmountInfo) -> Self {
        Self {
            description: description.as_ref().to_string(),
            out_trade_no: out_trade_no.as_ref().to_string(),
            amount,
            time_expire: None,
            attach: None,
            goods_tag: None,
            support_fapiao: None,
            scene_info: None,
            settle_info: None,
        }
    }
}

/// 微信支付回调通知的加密资源。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WechatPayNotifySource {
    /// 【加密算法】 对通知数据进行加密的算法。
    pub algorithm: String,
    /// 【密文】 通知数据的密文。
    pub ciphertext: String,
    /// 【附加数据】 参与解密的附加数据。
    pub associated_data: Option<String>,
    /// 【原始类型】 原始回调类型。
    pub original_type: String,
    /// 【随机串】 参与解密的随机串。
    pub nonce: String,
}

/// 微信回调 `resource` 固定使用的加密算法。
const NOTIFY_RESOURCE_ALGORITHM: &str = "AEAD_AES_256_GCM";

impl WechatPayNotifySource {
    /// 校验 `algorithm` 是本 SDK 支持的加密算法（`AEAD_AES_256_GCM`）。
    ///
    /// 微信只使用 AEAD_AES_256_GCM 加密回调的 `resource`。这道校验放在解密**之前**，
    /// 是为了让「算法不是 GCM」报成明确错误，而不是退化成 GCM 解密失败 —— 后者会把
    /// 「语义拒绝 / 需要升级 SDK」指向「密文或密钥不对」这个错误方向。
    pub fn validate_algorithm(&self) -> Result<(), crate::error::PayError> {
        if self.algorithm != NOTIFY_RESOURCE_ALGORITHM {
            return Err(crate::error::PayError::DecryptError(format!(
                "不支持的回调加密算法「{}」：微信回调的 resource 固定用 {NOTIFY_RESOURCE_ALGORITHM} 加密",
                self.algorithm
            )));
        }
        Ok(())
    }
}

/// 微信支付回调通知报文。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WechatPayNotify {
    /// 【通知ID】 微信支付回调通知的唯一标识。
    pub id: String,
    /// 【创建时间】 通知的创建时间。
    pub create_time: String,
    /// 【事件类型】 通知的事件类型。
    pub event_type: String,
    /// 【资源类型】 通知中的资源类型。
    pub resource_type: String,
    /// 【资源】 通知中的加密资源。
    pub resource: WechatPayNotifySource,
    /// 【摘要】 通知的摘要信息。
    pub summary: String,
}

/// 微信支付回调通知解密后的支付数据。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WechatPayDecodeData {
    /// 【商户号】 商户下单时的商户号。
    pub mchid: String,
    /// 【应用ID】 商户下单时的应用ID。
    pub appid: String,
    /// 【商户订单号】 商户系统内部订单号。
    pub out_trade_no: String,
    /// 【微信支付订单号】 微信支付侧订单的唯一标识。
    pub transaction_id: String,
    /// 【交易类型】 订单的交易类型。
    pub trade_type: String,
    /// 【交易状态】 交易状态，如 SUCCESS 等。
    pub trade_state: String,
    /// 【交易状态描述】 交易状态的文字描述。
    pub trade_state_desc: String,
    /// 【付款银行】 用户支付所使用的银行。
    pub bank_type: String,
    /// 【附加数据】 商户下单时传入的附加数据。**下单没传就不会返回**，因此是可选的。
    pub attach: Option<String>,
    /// 【支付完成时间】 交易支付完成的时间。
    pub success_time: String,
    /// 【支付者】 支付者信息。
    pub payer: PayerInfo,
    /// 【订单金额】 订单金额信息。
    pub amount: AmountInfo,
}

/// 退款结果通知解密后的数据（`REFUND.SUCCESS` / `REFUND.ABNORMAL` / `REFUND.CLOSED`）。
///
/// 退款通知的字段与支付通知**不重合**：没有 `appid` / `trade_state` / `payer`，多了
/// `out_refund_no` / `refund_id` / `refund_status`，所以不能复用 [`WechatPayDecodeData`]。
/// 解密入口是
/// [`PayNotifyTrait::decrypt_refund_paydata`](crate::pay::PayNotifyTrait::decrypt_refund_paydata)。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WechatPayRefundDecodeData {
    /// 【商户号】 商户下单时的商户号。
    pub mchid: String,
    /// 【商户订单号】 商户系统内部订单号。
    pub out_trade_no: String,
    /// 【微信支付订单号】 微信支付侧订单的唯一标识。
    pub transaction_id: String,
    /// 【商户退款单号】 商户系统内部的退款单号。
    pub out_refund_no: String,
    /// 【微信退款单号】 微信支付侧的退款单号。
    pub refund_id: String,
    /// 【退款状态】 `SUCCESS` / `CLOSED` / `ABNORMAL`。
    pub refund_status: String,
    /// 【退款成功时间】 仅退款成功时返回，格式同支付通知的 `success_time`。
    pub success_time: Option<String>,
    /// 【退款入账账户】 退回资金的入账账户。
    pub user_received_account: String,
    /// 【退款金额】 退款金额信息。
    pub amount: RefundNotifyAmount,
}

/// 退款结果通知里的金额信息（四项，与支付通知的 [`AmountInfo`] 不同）。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RefundNotifyAmount {
    /// 【订单金额】 订单总金额，单位为分。
    pub total: i32,
    /// 【退款金额】 本次退款金额，单位为分。
    pub refund: i32,
    /// 【用户支付金额】 用户实际支付的金额，单位为分。
    pub payer_total: i32,
    /// 【用户退款金额】 退给用户的金额，单位为分。
    pub payer_refund: i32,
}

/// 申请退款参数。
#[derive(Serialize, Debug, Clone)]
pub struct RefundsParams {
    /// 【微信支付订单号】 微信支付侧订单的唯一标识，订单支付成功后，查询订单和支付成功回调通知会返回该参数。
    /// transaction_id和out_trade_no必须二选一进行传参。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    /// 【商户订单号】 商户下单时传入的商户系统内部订单号。
    /// transaction_id和out_trade_no必须二选一进行传参。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_trade_no: Option<String>,
    /// 【商户退款单号】 商户系统内部的退款单号，商户系统内部唯一，
    /// 只能是数字、大小写字母_-|*@ ，
    /// 同一商户退款单号多次请求只退一笔。不可超过64个字节数。
    pub out_refund_no: String,
    /// 【退款原因】 若商户传了退款原因，该原因将在下发给用户的退款消息中显示，具体展示可参见退款通知UI示意图。
    /// 请注意：1、该退款原因参数的长度不得超过80个字节；2、当订单退款金额小于等于1元且为部分退款时，退款原因将不会在消息中体现。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 【退款结果回调url】 异步接收微信支付退款结果通知的回调地址，通知url必须为外网可访问的url，不能携带参数。
    /// 如果传了该参数，则商户平台上配置的回调地址（商户平台-交易中心-退款管理-退款配置）将不会生效，优先回调当前传的这个地址。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify_url: Option<String>,
    /// 【退款资金来源】 若传递此参数则使用对应的资金账户退款。
    /// 可选取值：
    /// AVAILABLE: 仅对旧资金流商户适用(请参考旧资金流介绍区分)，传此枚举指定从可用余额账户出资，否则默认使用未结算资金退款。
    /// UNSETTLED: 仅对出行预付押金退款适用，指定从未结算资金出资。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funds_account: Option<String>,
    /// 【金额信息】订单退款金额信息
    pub amount: RefundsAmountParams,
    /// 【退款商品】 请填写需要指定退款的商品信息，所指定的商品信息需要与下单时传入的单品列表goods_detail中的对应商品信息一致 ，如无需按照指定商品退款，本字段不填。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goods_detail: Option<Vec<RefundsGoodsDetailParams>>,
}

/// 退款金额信息。
#[derive(Serialize, Debug, Clone)]
pub struct RefundsAmountParams {
    /// 【退款金额】 退款金额，币种的最小单位，只能为整数，不能超过原订单支付金额。
    pub refund: i32,
    /// 【原订单金额】 原支付交易的订单总金额，币种的最小单位，只能为整数
    pub total: i32,
    /// 【退款币种】 符合ISO 4217标准的三位字母代码，固定传：CNY，代表人民币。
    pub currency: String,
    /// 【退款出资账户及金额】退款需从指定账户出资时，可传递该参数以指定出资金额（币种最小单位，仅限整数）。
    /// 多账户出资退款需满足：1、未开通退款支出分离功能；2、订单为待分账或分账中的分账订单。
    /// 传递参数需确保：1、基本账户可用与不可用余额之和等于退款金额；2、账户类型不重复。不符条件将返回错误。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Vec<RefundsFromParams>>,
}

/// 退款出资账户信息。
#[derive(Serialize, Debug, Clone)]
pub struct RefundsFromParams {
    /// 【出资账户类型】 退款出资的账户类型。
    /// AVAILABLE : 可用余额
    /// UNAVAILABLE : 不可用余额
    pub account: String,
    /// 【出资金额】对应账户出资金额
    pub amount: i32,
}

/// 退款商品信息。
#[derive(Serialize, Debug, Clone)]
pub struct RefundsGoodsDetailParams {
    /// 【商户侧商品编码】 订单下单时传入的商户侧商品编码。
    pub merchant_goods_id: String,
    /// 【微信侧商品编码】 订单下单时传入的微信侧商品编码（没有可不传）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wechatpay_goods_id: Option<String>,
    /// 【商品名称】 订单下单时传入的商品名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goods_name: Option<String>,
    /// 【商品单价】 订单下单时传入的商品单价。
    pub unit_price: i32,
    /// 【商品退款金额】 商品退款金额，单位为分
    pub refund_amount: i32,
    /// 【商品退货数量】 对应商品的退货数量
    pub refund_quantity: i32,
}

impl RefundsParams {
    /// 构造 RefundsParams。
    pub fn new<S: AsRef<str>>(
        out_refund_no: S,
        total: i32,
        refund: i32,
        transaction_id: Option<S>,
        out_trade_no: Option<S>,
    ) -> Self {
        let amount = RefundsAmountParams {
            refund,
            total,
            currency: Currency::CNY.to_string(),
            from: None,
        };

        Self {
            transaction_id: transaction_id.map(|s| s.as_ref().to_string()),
            out_trade_no: out_trade_no.map(|s| s.as_ref().to_string()),
            out_refund_no: out_refund_no.as_ref().to_string(),
            reason: None,
            notify_url: None,
            funds_account: None,
            amount,
            goods_detail: None,
        }
    }
}

impl ParamsTrait for RefundsParams {}

/// 交易账单类型（申请交易账单的 `bill_type`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillType {
    /// 当日所有订单（不含充值退款订单）
    All,
    /// 当日成功支付的订单
    Success,
    /// 当日退款订单
    Refund,
}

impl BillType {
    /// 官方取值（查询串里用的就是它）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Success => "SUCCESS",
            Self::Refund => "REFUND",
        }
    }
}

impl std::fmt::Display for BillType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 账单压缩类型（申请交易账单的 `tar_type`）。官方目前只支持 GZIP。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillTarType {
    /// `.gzip` 压缩后的账单文件流
    Gzip,
}

impl BillTarType {
    /// 官方取值
    pub fn as_str(&self) -> &'static str {
        "GZIP"
    }
}

impl std::fmt::Display for BillTarType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
