use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct NativeResponse {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【支付跳转链接】 h5_url为拉起微信支付收银台的中间页面，可通过访问该URL来拉起微信客户端，完成支付，h5_url的有效期为5分钟。
    pub code_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JsapiResponse {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【预支付交易会话标识】 预支付交易会话标识。用于后续接口调用中使用，该值有效期为2小时
    pub prepay_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AppResponse {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【预支付交易会话标识】 预支付交易会话标识。用于后续接口调用中使用，该值有效期为2小时
    pub prepay_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct H5Response {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【二维码链接】 此URL用于生成支付二维码，然后提供给用户扫码支付。
    /// 注意：code_url并非固定值，使用时按照URL格式转成二维码即可。
    pub h5_url: Option<String>,
}