#[derive(Debug, thiserror::Error)]
pub enum PayError {
    #[error("http error: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("pay error: {0}")]
    WechatError(String),
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Decrypt error: {0}")]
    DecryptError(String),
    #[error("Base64 decode error: {0}")]
    DecodeError(#[from] base64::DecodeError),
    #[error("verify error: {0}")]
    VerifyError(String),
    #[error("weixin not found error")]
    WeixinNotFound,
}
