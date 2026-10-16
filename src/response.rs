use crate::model::PayerInfo;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// 响应模型：可从微信返回的 JSON 反序列化。
pub trait ResponseTrait: DeserializeOwned {}

/// Native 下单响应：返回二维码链接 code_url。
#[derive(Debug, Deserialize)]
pub struct NativeResponse {
    ///【二维码链接】 此URL用于生成支付二维码，然后提供给用户扫码支付。
    /// 注意：code_url并非固定值，使用时按照URL格式转成二维码即可。
    pub code_url: Option<String>,
}

impl ResponseTrait for NativeResponse {}

/// JSAPI 下单响应：返回预支付交易会话标识 prepay_id 与签名数据。
#[derive(Debug, Deserialize)]
pub struct JsapiResponse {
    ///【预支付交易会话标识】 预支付交易会话标识。用于后续接口调用中使用，该值有效期为2小时
    pub prepay_id: Option<String>,
    ///【签名数据】
    pub sign_data: Option<SignData>,
}

/// JSAPI / 小程序签名数据：供 `wx.requestPayment` 拉起支付使用。
///
/// # 为什么字段名是 camelCase
///
/// `wx.requestPayment` 的参数名由官方定死：`timeStamp` / `nonceStr` / `package` /
/// `signType` / `paySign` —— 注意 **`timeStamp` 的 `S` 必须大写**。这里用
/// `#[serde(rename = …)]` 对齐官方，**Rust 侧的字段名保持 snake_case 不变**。
///
/// 键名写错时前端只会报「缺少参数」，**不会**指向这里，所以别手写这些名字，
/// 序列化本结构体即可（有测试钉住键名）。
///
/// - [`package`](Self::package) 在 JSAPI / 小程序 / 付款码路径下已经是带 `prepay_id=`
///   前缀的完整串，直接给前端；**APP 支付是裸 `prepay_id`**（见下方 ⚠）。
/// - [`app_id`](Self::app_id) 是**公众号 JSAPI** 才需要的（官方字段名 `appId`）；
///   **小程序**的 `wx.requestPayment` 参数表里没有这一项，前端忽略或删掉即可。
///
/// ⚠ **APP 支付不能复用这里的 JSON 键名**：APP SDK 的拉起参数是
/// `appid` / `partnerid` / `prepayid` / `package` / `noncestr` / `timestamp` / `sign`，
/// 与 JSAPI 完全不同，需要自行映射。
#[derive(Debug, Serialize, Deserialize)]
pub struct SignData {
    /// 【应用ID】 发起支付的公众号（或移动应用）的 appid。
    #[serde(rename = "appId")]
    pub app_id: String,
    /// 【签名类型】 签名算法类型，固定为 RSA。
    #[serde(rename = "signType")]
    pub sign_type: String,
    /// 【订单详情扩展字符串】 形如 `prepay_id=xxx`（APP 支付为裸 `prepay_id`），由预支付会话标识拼装。
    pub package: String,
    /// 【随机字符串】 参与签名的随机字符串。
    #[serde(rename = "nonceStr")]
    pub nonce_str: String,
    /// 【时间戳】 参与签名的秒级时间戳。
    #[serde(rename = "timeStamp")]
    pub timestamp: String,
    /// 【签名值】 使用商户私钥对上述参数生成的签名。
    #[serde(rename = "paySign")]
    pub pay_sign: String,
}

impl ResponseTrait for JsapiResponse {}

/// APP 下单响应：返回预支付交易会话标识 prepay_id 与签名数据。
#[derive(Debug, Deserialize)]
pub struct AppResponse {
    ///【预支付交易会话标识】 预支付交易会话标识。用于后续接口调用中使用，该值有效期为2小时
    pub prepay_id: Option<String>,
    ///【签名数据】
    pub sign_data: Option<SignData>,
}

impl ResponseTrait for AppResponse {}

/// 付款码下单响应：返回预支付交易会话标识 prepay_id 与签名数据。
#[derive(Debug, Deserialize)]
pub struct MicroResponse {
    ///【预支付交易会话标识】 预支付交易会话标识。用于后续接口调用中使用，该值有效期为2小时
    pub prepay_id: Option<String>,
    ///【签名数据】
    pub sign_data: Option<SignData>,
}

impl ResponseTrait for MicroResponse {}

/// H5 下单响应：返回拉起微信收银台的 h5_url。
#[derive(Debug, Deserialize)]
pub struct H5Response {
    ///【支付跳转链接】 h5_url为拉起微信支付收银台的中间页面，可通过访问该URL来拉起微信客户端，完成支付，h5_url的有效期为5分钟。
    pub h5_url: Option<String>,
}

impl ResponseTrait for H5Response {}

/// 平台证书的加密信息。
#[derive(Debug, Clone, Deserialize)]
pub struct EncryptCertificate {
    /// 【加密算法】 证书密文使用的加密算法，固定为 AEAD_AES_256_GCM。
    pub algorithm: String,
    /// 【随机串】 加密使用的随机串。
    pub nonce: String,
    /// 【附加数据】 加密使用的附加数据。
    pub associated_data: String,
    /// 【密文】 证书内容的 Base64 密文。
    pub ciphertext: String,
}

/// 微信支付平台证书。
#[derive(Debug, Clone, Deserialize)]
pub struct Certificate {
    /// 【证书序列号】 平台证书的序列号。
    pub serial_no: String,
    /// 【生效时间】 证书生效时间，遵循 rfc3339 格式。
    pub effective_time: String,
    /// 【失效时间】 证书失效时间，遵循 rfc3339 格式。
    pub expire_time: String,
    /// 【加密证书】 证书的加密信息。
    pub encrypt_certificate: EncryptCertificate,
}

/// 获取平台证书响应：GET /v3/certificates 返回的证书列表。
#[derive(Debug, Deserialize)]
pub struct CertificateResponse {
    /// 【证书列表】 微信支付平台证书列表。
    pub data: Option<Vec<Certificate>>,
}

impl ResponseTrait for CertificateResponse {}

/// 微信支付错误响应：包含错误码与字段级错误详情。
#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    /// 【错误码】 错误码
    pub code: Option<String>,
    /// 【错误信息】 错误信息
    pub message: Option<String>,
    /// 【错误详情】 微信返回的字段级错误定位信息，原样保留便于排查。
    pub detail: Option<serde_json::Value>,
}

impl std::fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "code={} message={}",
            self.code.as_deref().unwrap_or("<none>"),
            self.message.as_deref().unwrap_or("<none>"),
        )?;
        if let Some(detail) = &self.detail {
            write!(f, " detail={detail}")?;
        }
        Ok(())
    }
}

/// 退款单响应：退款申请受理后返回的退款单信息。
#[derive(Debug, Deserialize)]
pub struct RefundsResponse {
    /// 【微信支付退款单号】申请退款受理成功时，该笔退款单在微信支付侧生成的唯一标识。
    pub refund_id: String,
    /// 【商户退款单号】 商户申请退款时传的商户系统内部退款单号。
    pub out_refund_no: String,
    /// 【微信支付订单号】微信支付侧订单的唯一标识。
    pub transaction_id: String,
    /// 【商户订单号】 商户下单时传入的商户系统内部订单号。
    pub out_trade_no: String,
    /// 【退款渠道】 订单退款渠道
    /// 以下枚举：
    /// ORIGINAL: 原路退款
    /// BALANCE: 退回到余额
    /// OTHER_BALANCE: 原账户异常退到其他余额账户
    /// OTHER_BANKCARD: 原银行卡异常退到其他银行卡(发起异常退款成功后返回)
    pub channel: String,
    /// 【退款入账账户】 取当前退款单的退款入账方，有以下几种情况：
    /// 1）退回银行卡：{银行名称}{卡类型}{卡尾号}
    /// 2）退回支付用户零钱:支付用户零钱
    /// 3）退还商户:商户基本账户商户结算银行账户
    /// 4）退回支付用户零钱通:支付用户零钱通
    /// 5）退回支付用户银行电子账户:支付用户银行电子账户
    /// 6）退回支付用户零花钱:支付用户零花钱
    /// 7）退回用户经营账户:用户经营账户
    /// 8）退回支付用户来华零钱包:支付用户来华零钱包
    /// 9）退回企业支付商户:企业支付商户
    pub user_received_account: String,
    /// 【退款成功时间】
    /// 1、定义：退款成功的时间，该字段在退款状态status为SUCCESS（退款成功）时返回。
    /// 2、格式：遵循rfc3339标准格式：yyyy-MM-DDTHH:mm:ss+TIMEZONE。yyyy-MM-DD 表示年月日；T 字符用于分隔日期和时间部分；HH:mm:ss 表示具体的时分秒；TIMEZONE 表示时区（例如，+08:00 对应东八区时间，即北京时间）。
    /// 示例：2015-05-20T13:29:35+08:00 表示北京时间2015年5月20日13点29分35秒。
    pub success_time: Option<String>,
    /// 【退款创建时间】
    /// 1、定义：提交退款申请成功，微信受理退款申请单的时间。
    /// 2、格式：遵循rfc3339标准格式：yyyy-MM-DDTHH:mm:ss+TIMEZONE。yyyy-MM-DD 表示年月日；T 字符用于分隔日期和时间部分；HH:mm:ss 表示具体的时分秒；TIMEZONE 表示时区（例如，+08:00 对应东八区时间，即北京时间）。
    /// 示例：2015-05-20T13:29:35+08:00 表示北京时间2015年5月20日13点29分35秒。
    pub create_time: String,
    /// 【退款状态】退款单的退款处理状态。
    /// SUCCESS: 退款成功
    /// CLOSED: 退款关闭
    /// PROCESSING: 退款处理中
    /// ABNORMAL: 退款异常，退款到银行发现用户的卡作废或者冻结了，导致原路退款银行卡失败，可前往商户平台-交易中心，手动处理此笔退款，可参考： 退款异常的处理，或者通过发起异常退款接口进行处理。
    /// 注：状态流转说明请参考状态流转图
    pub status: String,
    /// 【资金账户】 退款所使用资金对应的资金账户类型
    /// UNSETTLED: 未结算资金
    /// AVAILABLE: 可用余额
    /// UNAVAILABLE: 不可用余额
    /// OPERATION: 运营账户
    /// BASIC: 基本账户（含可用余额和不可用余额）
    /// ECNY_BASIC: 数字人民币基本账户
    pub funds_account: String,
    /// 【金额信息】订单退款金额信息
    pub amount: RefundsAmountResponse,
    /// 【优惠退款详情】 订单各个代金券的退款详情，订单使用了代金券且代金券发生退款时返回。
    pub promotion_detail: Option<Vec<RefundsPromotionDetailResponse>>,
}

impl ResponseTrait for RefundsResponse {}

/// 退款金额信息。
#[derive(Debug, Deserialize)]
pub struct RefundsAmountResponse {
    /// 【订单金额】 订单总金额，单位为分
    pub total: i32,
    /// 【退款金额】退款金额，单位为分，只能为整数，可以做部分退款，不能超过原订单支付金额。
    pub refund: i32,
    /// 【退款出资账户及金额】 退款出资的账户类型及金额信息，若此接口请求时未传该参数，则不会返回。
    pub from: Option<Vec<RefundsFromResponse>>,
    /// 【用户实际支付金额】用户现金支付金额，整型，单位为分，例如10元订单用户使用了2元全场代金券，则该金额为用户实际支付的8元。
    pub payer_total: i32,
    /// 【用户退款金额】 指用户实际收到的现金退款金额，数据类型为整型，单位为分。例如在一个10元的订单中，用户使用了2元的全场代金券，若商户申请退款5元，则用户将收到4元的现金退款(即该字段所示金额)和1元的代金券退款。
    /// 注：部分退款用户无法继续使用代金券，只有在订单全额退款且代金券未过期的情况下，且全场券属于银行立减金用户才能继续使用代金券。
    /// 详情参考含优惠退款说明。
    pub payer_refund: i32,
    /// 【应结退款金额】 去掉免充值代金券退款金额后的退款金额，整型，单位为分，例如10元订单用户使用了2元全场代金券(一张免充值1元 + 一张预充值1元)，商户申请退款5元，则该金额为 退款金额5元 - 0.5元免充值代金券退款金额 = 4.5元。
    pub settlement_refund: i32,
    /// 【应结订单金额】去除免充值代金券金额后的订单金额，整型，单位为分，例如10元订单用户使用了2元全场代金券(一张免充值1元 + 一张预充值1元)，则该金额为 订单金额10元 - 免充值代金券金额1元 = 9元。
    pub settlement_total: i32,
    /// 【优惠退款金额】 申请退款后用户收到的代金券退款金额，整型，单位为分，例如10元订单用户使用了2元全场代金券，商户申请退款5元，用户收到的是4元现金 + 1元代金券退款金额(该字段) 。
    pub discount_refund: i32,
    /// 【退款币种】 固定返回：CNY，代表人民币。
    pub currency: String,
    /// 【手续费退款金额】 订单退款时退还的手续费金额，整型，单位为分，例如一笔100元的订单收了0.6元手续费，商户申请退款50元，该金额为等比退还的0.3元手续费。
    pub refund_fee: Option<i32>,
}

/// 退款出资账户及金额信息。
#[derive(Debug, Deserialize)]
pub struct RefundsFromResponse {
    /// 【出资账户类型】下面枚举值多选一。
    /// 枚举值：
    /// AVAILABLE : 可用余额
    /// UNAVAILABLE : 不可用余额
    pub account: String,
    /// 【出资金额】 对应账户出资金额，单位为分
    pub amount: i32,
}

/// 退款优惠详情：代金券退款信息。
#[derive(Debug, Deserialize)]
pub struct RefundsPromotionDetailResponse {
    /// 【券ID】代金券id，单张代金券的编号
    pub promotion_id: String,
    /// 【优惠范围】优惠活动中代金券的适用范围，分为两种类型：
    /// GLOBAL：全场代金券-以订单整体可优惠的金额为优惠门槛的代金券；
    /// SINGLE：单品优惠-以订单中具体某个单品的总金额为优惠门槛的代金券
    pub scope: String,
    /// 【优惠类型】代金券资金类型，优惠活动中代金券的结算资金类型，分为两种类型：
    /// CASH：预充值-带有结算资金的代金券，会随订单结算给订单收款商户；
    /// NOCASH：免充值-不带有结算资金的代金券，无资金结算给订单收款商户。
    pub r#type: String,
    /// 【代金券面额】 代金券优惠的金额
    pub amount: i32,
    /// 【优惠退款金额】 代金券退款的金额
    pub refund_amount: i32,
    /// 【退款商品】 指定商品退款时传的退款商品信息。
    pub goods_detail: Option<Vec<RefundsGoodsDetailResponse>>,
}

/// 退款商品详情：指定商品退款时的商品信息。
#[derive(Debug, Deserialize)]
pub struct RefundsGoodsDetailResponse {
    /// 【商户侧商品编码】 申请退款的商户侧商品编码。
    pub merchant_goods_id: Option<String>,
    /// 【微信侧商品编码】 申请退款的微信侧商品编码。（申请退款时没传则不返回）
    pub wechatpay_goods_id: Option<String>,
    /// 【商品名称】 申请退款的商品名称。（申请退款时没传则不返回）
    pub goods_name: Option<String>,
    /// 【商品单价】 申请退款的商品单价。
    pub unit_price: i32,
    /// 【商品退款金额】 申请退款的商品退款金额。
    pub refund_amount: i32,
    /// 【商品退货数量】 申请退款的商品退货数量。
    pub refund_quantity: i32,
}

/// 交易订单响应：查询订单返回的交易详情。
#[derive(Debug, Deserialize)]
pub struct TransactionResponse {
    /// 【应用ID】 直连商户申请的公众号或移动应用appid。
    pub appid: String,
    /// 【直连商户号】 直连商户的商户号，由微信支付生成并下发。
    pub mchid: String,
    /// 【商户订单号】 商户系统内部订单号，只能是数字、大小写字母_-*且在同一个商户号下唯一。
    pub out_trade_no: String,
    /// 【交易状态】 交易状态
    /// SUCCESS：支付成功
    /// REFUND：转入退款
    /// NOTPAY：未支付
    /// CLOSED：已关闭
    /// REVOKED：已撤销（仅付款码支付会返回）
    /// USERPAYING：用户支付中（仅付款码支付会返回）
    /// PAYERROR：支付失败（仅付款码支付会返回）
    pub trade_state: String,
    /// 【交易状态描述】 交易状态描述
    pub trade_state_desc: String,
    /// 【微信支付订单号】 微信支付侧订单的唯一标识，订单支付成功后返回。
    pub transaction_id: Option<String>,
    /// 【交易类型】 交易类型
    /// JSAPI：公众号支付
    /// NATIVE：扫码支付
    /// APP：APP支付
    /// MICROPAY：付款码支付
    /// MWEB：H5支付
    /// FACEPAY：刷脸支付
    pub trade_type: Option<String>,
    /// 【付款银行】 银行类型，采用字符串类型的银行标识。
    pub bank_type: Option<String>,
    /// 【附加数据】 附加数据，在查询API和支付通知中原样返回，可作为自定义参数使用，实际情况下只有支付完成状态才会返回该字段。
    pub attach: Option<String>,
    /// 【支付完成时间】 支付完成时间，遵循rfc3339标准格式：yyyy-MM-DDTHH:mm:ss+TIMEZONE。
    pub success_time: Option<String>,
    /// 【支付者】 支付者信息。
    pub payer: Option<PayerInfo>,
    /// 【订单金额】 订单金额信息。
    pub amount: Option<TransactionAmountResponse>,
    /// 【场景信息】 支付场景描述。
    pub scene_info: Option<TransactionSceneInfo>,
    /// 【优惠功能】 订单优惠信息，订单使用了代金券时返回。
    pub promotion_detail: Option<Vec<TransactionPromotionDetail>>,
}

impl ResponseTrait for TransactionResponse {}

/// 交易订单金额信息。
#[derive(Debug, Deserialize)]
pub struct TransactionAmountResponse {
    /// 【订单金额】 订单总金额，单位为分。
    pub total: Option<i32>,
    /// 【用户支付金额】 用户支付金额，单位为分。
    pub payer_total: Option<i32>,
    /// 【货币类型】 货币类型，固定返回CNY，代表人民币。
    pub currency: Option<String>,
    /// 【用户支付币种】 用户支付币种，固定返回CNY，代表人民币。
    pub payer_currency: Option<String>,
}

/// 交易场景信息。
#[derive(Debug, Deserialize)]
pub struct TransactionSceneInfo {
    /// 【商户端设备号】 商户端设备号（门店号或收银设备ID）。
    pub device_id: Option<String>,
}

/// 交易优惠详情：代金券优惠信息。
#[derive(Debug, Deserialize)]
pub struct TransactionPromotionDetail {
    /// 【券ID】 代金券id，单张代金券的编号。
    pub coupon_id: String,
    /// 【优惠名称】 优惠名称。
    pub name: String,
    /// 【优惠金额】 优惠金额，单位为分。
    pub amount: i32,
    /// 【优惠范围】 优惠活动中代金券的适用范围
    /// GLOBAL：全场代金券
    /// SINGLE：单品优惠
    pub scope: Option<String>,
    /// 【优惠类型】 代金券资金类型
    /// CASH：预充值
    /// NOCASH：免充值
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    /// 【活动ID】 优惠活动ID。
    pub stock_id: Option<String>,
    /// 【微信出资】 微信出资，单位为分。
    pub wechatpay_contribute: Option<i32>,
    /// 【商户出资】 商户出资，单位为分。
    pub merchant_contribute: Option<i32>,
    /// 【其他出资】 其他出资，单位为分。
    pub other_contribute: Option<i32>,
    /// 【优惠币种】 优惠币种，固定返回CNY，代表人民币。
    pub currency: Option<String>,
    /// 【单品列表】 单品优惠信息。
    pub goods_detail: Option<Vec<TransactionPromotionGoodsDetail>>,
}

/// 交易优惠商品详情：单品优惠信息。
#[derive(Debug, Deserialize)]
pub struct TransactionPromotionGoodsDetail {
    /// 【商品编码】 商品编码。
    pub goods_id: String,
    /// 【商品数量】 用户购买的数量。
    pub quantity: i32,
    /// 【商品单价】 商品单价，单位为分。
    pub unit_price: i32,
    /// 【商品优惠金额】 商品优惠金额，单位为分。
    pub discount_amount: i32,
    /// 【商品备注】 商品备注。
    pub goods_remark: Option<String>,
}

/// 申请交易账单响应（`GET /v3/bill/tradebill`）：账单文件的下载地址与完整性摘要。
///
/// ⚠ 官方给的摘要算法是 **SHA1**（`hash_type` 固定 `SHA1`），且账单**下载**应答不带签名头 ——
/// 完整性只能靠调用方比对 `hash_value`，`WechatPay::download_bill` 不做验签（也没有可验的东西）。
#[derive(Debug, Deserialize)]
pub struct TradeBillResponse {
    ///【摘要类型】 固定为 `SHA1`
    pub hash_type: String,
    ///【摘要值】 账单文件的 SHA1 摘要（十六进制），用于校验文件一致性
    pub hash_value: String,
    ///【下载地址】 下一步下载账单文件的地址（官方：5 分钟内有效）
    pub download_url: String,
}

impl ResponseTrait for TradeBillResponse {}
