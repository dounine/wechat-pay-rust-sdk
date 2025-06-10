use base64::engine::general_purpose;
use base64::{DecodeError, Engine};
use std::error::Error;
use uuid::Uuid;

pub fn random_trade_no() -> String {
    Uuid::new_v4().simple().to_string()
}

pub fn base64_encode<S>(content: S) -> String
where
    S: AsRef<[u8]>,
{
    general_purpose::STANDARD.encode(content)
}

pub fn base64_decode<S>(content: S) -> Result<Vec<u8>, DecodeError>
where
    S: AsRef<[u8]>,
{
    general_purpose::STANDARD.decode(content.as_ref())
}

pub fn x509_to_pem(content: &[u8]) -> Result<String, Box<dyn Error>> {
    let pem = pem::parse(content)?;
    let (_, cert) = x509_parser::parse_x509_certificate(pem.contents())?;
    let pub_key = base64_encode(cert.public_key().raw);
    let pub_key_lines = pub_key
        .chars()
        .collect::<Vec<char>>()
        .chunks(64)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<String>>()
        .join("\n");
    Ok(format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
        pub_key_lines
    ))
}

pub fn x509_is_valid(content: &[u8]) -> Result<(bool, i64), Box<dyn Error>> {
    let pem = pem::parse(content)?;
    let (_, cert) = x509_parser::parse_x509_certificate(pem.contents())?;
    //读取到证书的有效期
    let expire_time = cert.validity().is_valid();
    Ok((expire_time, cert.validity.not_after.timestamp()))
}
