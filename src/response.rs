use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct NativeResponse {
    pub code: Option<String>,
    pub message: Option<String>,
    ///【二维码链接】 此URL用于生成支付二维码，然后提供给用户扫码支付。
    /// 注意：code_url并非固定值，使用时按照URL格式转成二维码即可。
    pub code_url: Option<String>,
}