use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub trait ResponseTrait: DeserializeOwned {}

#[derive(Debug, Deserialize)]
pub struct NativeResponse {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【支付跳转链接】 h5_url为拉起微信支付收银台的中间页面，可通过访问该URL来拉起微信客户端，完成支付，h5_url的有效期为5分钟。
    pub code_url: Option<String>,
}

impl ResponseTrait for NativeResponse {}

#[derive(Debug, Deserialize)]
pub struct JsapiResponse {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【预支付交易会话标识】 预支付交易会话标识。用于后续接口调用中使用，该值有效期为2小时
    pub prepay_id: Option<String>,
    ///【签名数据】
    pub sign_data: Option<SignData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignData {
    pub app_id: String,
    pub sign_type: String,
    pub package: String,
    pub nonce_str: String,
    pub timestamp: String,
    pub pay_sign: String,
}

impl ResponseTrait for JsapiResponse {}

#[derive(Debug, Deserialize)]
pub struct AppResponse {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【预支付交易会话标识】 预支付交易会话标识。用于后续接口调用中使用，该值有效期为2小时
    pub prepay_id: Option<String>,
    ///【签名数据】
    pub sign_data: Option<SignData>,
}

impl ResponseTrait for AppResponse {}

#[derive(Debug, Deserialize)]
pub struct MicroResponse {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【预支付交易会话标识】 预支付交易会话标识。用于后续接口调用中使用，该值有效期为2小时
    pub prepay_id: Option<String>,
    ///【签名数据】
    pub sign_data: Option<SignData>,
}

impl ResponseTrait for MicroResponse {}

#[derive(Debug, Deserialize)]
pub struct H5Response {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【二维码链接】 此URL用于生成支付二维码，然后提供给用户扫码支付。
    /// 注意：code_url并非固定值，使用时按照URL格式转成二维码即可。
    pub h5_url: Option<String>,
}

impl ResponseTrait for H5Response {}

#[derive(Debug, Clone, Deserialize)]
pub struct EncryptCertificate {
    pub algorithm: String,
    pub nonce: String,
    pub associated_data: String,
    pub ciphertext: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Certificate {
    pub serial_no: String,
    pub effective_time: String,
    pub expire_time: String,
    pub encrypt_certificate: EncryptCertificate,
}

#[derive(Debug, Deserialize)]
pub struct CertificateResponse {
    pub data: Option<Vec<Certificate>>,
}

impl ResponseTrait for CertificateResponse {}

#[derive(Deserialize, Debug)]
#[serde(untagged, bound = "T: ResponseTrait + DeserializeOwned")]
pub enum WeChatResponse<T>
where
    T: ResponseTrait + DeserializeOwned,
{
    Ok(T),
    Err(ErrorResponse),
}

impl<T> ResponseTrait for WeChatResponse<T> where T: ResponseTrait + DeserializeOwned {}

impl<T> WeChatResponse<T>
where
    T: ResponseTrait + DeserializeOwned,
{
    pub fn is_success(&self) -> bool {
        matches!(self, WeChatResponse::Ok(_))
    }

    pub fn ok(&self) -> Option<&T> {
        if let WeChatResponse::Ok(response) = self {
            Some(response)
        } else {
            None
        }
    }

    pub fn err(&self) -> Option<&ErrorResponse> {
        if let WeChatResponse::Err(error_response) = self {
            Some(error_response)
        } else {
            None
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    /// 【错误码】 错误码
    pub code: Option<String>,
    /// 【错误信息】 错误信息
    pub message: Option<String>,
}

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
