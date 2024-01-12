use base64::{DecodeError, Engine};
use base64::engine::general_purpose;
use uuid::Uuid;

pub fn random_trade_no() -> String {
    Uuid::new_v4().simple().to_string()
}

pub fn base64_encode<S>(content: S) -> String
    where S: AsRef<[u8]>
{
    general_purpose::STANDARD.encode(content)
}

pub fn base64_decode<S>(content: S) -> Result<Vec<u8>, DecodeError>
    where S: AsRef<[u8]>
{
    general_purpose::STANDARD.decode(content.as_ref())
}