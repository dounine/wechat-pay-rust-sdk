#[derive(Debug, thiserror::Error)]
pub enum PayError {
    #[error("http error: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("pay error: {0}")]
    WechatError(String),
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
}