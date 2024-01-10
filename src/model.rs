use std::fmt::{Display, Formatter};
use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
pub enum Currency {
    CNY,
}

unsafe impl Send for Currency {}

unsafe impl Sync for Currency {}

impl Display for Currency {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Currency::CNY => write!(f, "CNY"),
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct AmountInfo {
    ///【标价金额】 订单总金额，单位为分。
    pub total: i32,
    ///【标价币种】 符合ISO 4217标准的三位字母代码，默认人民币：CNY。
    pub currency: Currency,
}

impl From<i32> for AmountInfo {
    fn from(value: i32) -> Self {
        Self {
            total: value,
            currency: Currency::CNY,
        }
    }
}

unsafe impl Send for AmountInfo {}

unsafe impl Sync for AmountInfo {}

#[derive(Serialize, Debug, Clone)]
pub struct PayerInfo {
    ///【用户标识】 用户在直连商户appid下的唯一标识。
    pub openid: String,
}

unsafe impl Send for PayerInfo {}

unsafe impl Sync for PayerInfo {}

#[derive(Serialize, Debug, Clone)]
pub struct GoodsDetail {
    ///【商户侧商品编码】 由半角的大小写字母、数字、中划线、下划线中的一种或几种组成。
    pub merchant_goods_id: String,
    ///【商品数量】 用户购买的数量
    pub quantity: i32,
    ///【商品单价】 单位为：分。如果商户有优惠，需传输商户优惠后的单价(例如：用户对一笔100元的订单使用了商场发的纸质优惠券100-50，则活动商品的单价应为原单价-50)
    pub unit_price: i32,
    ///【微信支付商品编码】 微信支付定义的统一商品编号（没有可不传）
    pub wechatpay_goods_id: Option<String>,
    ///【商品名称】 商品的实际名称
    pub goods_name: Option<String>,
}

unsafe impl Send for GoodsDetail {}

unsafe impl Sync for GoodsDetail {}

#[derive(Serialize, Debug, Clone)]
pub struct OrderDetail {
    ///【订单原价】
    /// 1、商户侧一张小票订单可能被分多次支付，订单原价用于记录整张小票的交易金额。
    /// 2、当订单原价与支付金额不相等，则不享受优惠。
    /// 3、该字段主要用于防止同一张小票分多次支付，以享受多次优惠的情况，正常支付订单不必上传此参数。
    pub cost_price: Option<i32>,
    ///【商品小票ID】 商家小票ID
    pub invoice_id: Option<String>,
    ///【单品列表】 单品列表信息,条目个数限制：【1，6000】
    pub goods_detail: Vec<GoodsDetail>,
}

unsafe impl Send for OrderDetail {}

unsafe impl Sync for OrderDetail {}

#[derive(Serialize, Debug, Clone)]
pub struct StoreInfo {
    ///【门店编号】 商户侧门店编号
    pub id: String,
    ///【门店名称】 商户侧门店名称
    pub name: Option<String>,
    ///【地区编码】 地区编码，详细请见省市区编号对照表。
    pub area_code: Option<String>,
    ///【详细地址】 详细的商户门店地址
    pub address: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct SceneInfo {
    ///【用户终端IP】 用户的客户端IP，支持IPv4和IPv6两种格式的IP地址。
    pub payer_client_ip: String,
    ///【商户端设备号】 商户端设备号（门店号或收银设备ID）。
    pub device_id: Option<String>,
    ///【商户门店信息】 商户门店信息
    pub store_info: Option<StoreInfo>,
}

unsafe impl Send for SceneInfo {}

unsafe impl Sync for SceneInfo {}

#[derive(Serialize, Debug, Clone)]
pub struct JsapiConfig {
    ///【商品描述】 商品描述
    pub description: String,
    ///【商户订单号】 商户系统内部订单号，只能是数字、大小写字母_-*且在同一个商户号下唯一。
    pub out_trade_no: String,
    ///【订单金额】 订单金额信息
    pub amount: AmountInfo,
    ///【支付者】 支付者信息
    pub payer: PayerInfo,
    ///【通知地址】 异步接收微信支付结果通知的回调地址，通知URL必须为外网可访问的URL，不能携带参数。 公网域名必须为HTTPS，如果是走专线接入，使用专线NAT IP或者私有回调域名可使用HTTP
    pub notify_url: String,
    ///【附加数据】 附加数据，在查询API和支付通知中原样返回，可作为自定义参数使用，实际情况下只有支付完成状态才会返回该字段。
    pub attach: Option<String>,
    ///【优惠功能】 优惠功能
    pub detail: Option<OrderDetail>,
    ///【交易结束时间】 订单失效时间，遵循rfc3339标准格式，格式为yyyy-MM-DDTHH:mm:ss+TIMEZONE，yyyy-MM-DD表示年月日，T出现在字符串中，表示time元素的开头，HH:mm:ss表示时分秒，TIMEZONE表示时区（+08:00表示东八区时间，领先UTC8小时，即北京时间）。例如：2015-05-20T13:29:35+08:00表示，北京时间2015年5月20日13点29分35秒。
    pub time_expire: Option<i64>,
    ///【场景信息】 支付场景描述
    pub scene_info: Option<SceneInfo>,
}

#[derive(Serialize, Debug, Clone)]
pub struct SettleInfo {
    ///【是否指定分账】 是否指定分账，
    pub profit_sharing: Option<bool>,
}

unsafe impl Send for SettleInfo {}

unsafe impl Sync for SettleInfo {}

#[derive(Serialize, Debug, Clone)]
pub struct NativeConfig {
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
    pub time_expire: Option<i64>,
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

impl NativeConfig {
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

unsafe impl Send for NativeConfig {}

unsafe impl Sync for NativeConfig {}


unsafe impl Send for JsapiConfig {}

unsafe impl Sync for JsapiConfig {}