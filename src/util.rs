use base64::engine::general_purpose;
use base64::{DecodeError, Engine};
use rsa::pkcs8::DecodePublicKey;
use rsa::sha2::{Digest, Sha256};
use rsa::{Pkcs1v15Sign, RsaPublicKey};
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::error::PayError;

/// 用 PEM 公钥验证 RSA-SHA256（PKCS#1 v1.5）签名，`signature_b64` 为 base64。
///
/// 微信支付 v3 的应答验签与回调验签都走这条路径；平台证书轮换时由调用方
/// 先从证书列表里挑出对应公钥再传进来。
///
/// ⚠ 每次调用都会**重新解析** `pub_key_pem`。要「解析一次、验签多次」（平台证书索引的
/// 热路径），crate 内部用 `parse_rsa_public_key` + `verify_rsa_sha256_with_key`。
pub fn verify_rsa_sha256(
    pub_key_pem: &str,
    message: &str,
    signature_b64: &str,
) -> Result<(), PayError> {
    let pub_key = parse_rsa_public_key(pub_key_pem)?;
    verify_rsa_sha256_with_key(&pub_key, message, signature_b64)
}

/// 解析 PEM 公钥（SubjectPublicKeyInfo）。
///
/// 单拆出来是为了让调用方能缓存解析结果：`PlatformKeys` 的索引对每个 serial
/// 只解析一次。失败映射与 [`verify_rsa_sha256`] 一致（`VerifyError`）。
pub(crate) fn parse_rsa_public_key(pem: &str) -> Result<RsaPublicKey, PayError> {
    #[cfg(test)]
    RSA_PEM_PARSE_COUNT.with(|count| count.set(count.get() + 1));
    RsaPublicKey::from_public_key_pem(pem)
        .map_err(|e| PayError::VerifyError(format!("public key parser error: {e}")))
}

/// 用**已解析**的公钥验证 RSA-SHA256（PKCS#1 v1.5）签名，`signature_b64` 为 base64。
pub(crate) fn verify_rsa_sha256_with_key(
    pub_key: &RsaPublicKey,
    message: &str,
    signature_b64: &str,
) -> Result<(), PayError> {
    let hashed = Sha256::new().chain_update(message).finalize();
    let signature = base64_decode(signature_b64)?;
    pub_key
        .verify(Pkcs1v15Sign::new::<Sha256>(), &hashed, signature.as_slice())
        .map_err(|e| PayError::VerifyError(e.to_string()))
}

// 公钥 PEM 的解析次数（仅测试用）。
//
// `PlatformKeys`「同一把钥匙只解析一次」这条性质靠它证明；用线程局部量而不是全局
// 原子量，免得与并行的其它单测互相污染计数。
// （这里是普通注释而非文档注释：`///` 挂在 `thread_local!` 宏调用上会被判为
// unused doc comment，而 CI 的 `-D warnings` 会把它变成硬错误。）
#[cfg(test)]
thread_local! {
    static RSA_PEM_PARSE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// 当前线程累计解析公钥 PEM 的次数（仅测试用）。
#[cfg(test)]
pub(crate) fn rsa_pem_parse_count() -> usize {
    RSA_PEM_PARSE_COUNT.with(std::cell::Cell::get)
}

/// 生成一个随机的商户订单号：UUID v4 去掉连字符后的 32 位十六进制串。
pub fn random_trade_no() -> String {
    Uuid::new_v4().simple().to_string()
}

/// 当前 Unix 时间戳（秒）。
///
/// 签名串里的 `timestamp` 字段与回调通知的防重放校验都用它取「现在」。
/// 直接读系统时钟；若系统时钟早于 1970-01-01（`duration_since` 返回 `Err`），
/// 返回 0 而不是 panic。
pub fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Base64 编码（STANDARD 字母表，带 padding）。
///
/// 微信的签名、密文、证书都走这一套，不要换成 URL-safe 字母表。
pub fn base64_encode<S>(content: S) -> String
where
    S: AsRef<[u8]>,
{
    general_purpose::STANDARD.encode(content)
}

/// Base64 解码（STANDARD 字母表）。
pub fn base64_decode<S>(content: S) -> Result<Vec<u8>, DecodeError>
where
    S: AsRef<[u8]>,
{
    general_purpose::STANDARD.decode(content.as_ref())
}

/// 把平台的 **PEM 证书**转换为 PEM 格式的**公钥**。
///
/// 用于 `GET /v3/certificates`：`encrypt_certificate.ciphertext` 解密出来的证书是
/// **PEM**（不是 DER），这里取出其中的 SubjectPublicKeyInfo，再重新包成
/// `-----BEGIN PUBLIC KEY-----`，供 [`verify_rsa_sha256`] 验签。
/// 换行由 [`pem::encode_config`] 按 LF、每行 64 列输出。
pub fn x509_to_pem(content: &[u8]) -> Result<String, Box<dyn Error>> {
    let cert_pem = pem::parse(content)?;
    let (_, cert) = x509_parser::parse_x509_certificate(cert_pem.contents())?;
    let pub_key = pem::Pem::new("PUBLIC KEY", cert.public_key().raw);
    let config = pem::EncodeConfig::new().set_line_ending(pem::LineEnding::LF);
    Ok(pem::encode_config(&pub_key, config))
}

/// 返回 `(证书当前是否有效, not_after 的 unix 秒时间戳)`。
///
/// 用于平台证书的到期过滤：`GET /v3/certificates` 的响应里可能带着已过期的证书
/// （轮换期新旧两张都在有效期内时两张都会保留，过期的则应当丢弃）。
///
/// 解析失败返回 [`PayError::VerifyError`]：与库其余错误类型一致，调用方可以直接 `?`。
pub fn x509_is_valid(content: &[u8]) -> Result<(bool, i64), PayError> {
    fn invalid(error: impl std::fmt::Display) -> PayError {
        PayError::VerifyError(format!("平台证书解析失败: {error}"))
    }
    let pem = pem::parse(content).map_err(invalid)?;
    let (_, cert) = x509_parser::parse_x509_certificate(pem.contents()).map_err(invalid)?;
    // 读取到证书的有效期
    let expire_time = cert.validity().is_valid();
    Ok((expire_time, cert.validity.not_after.timestamp()))
}

/// 账单日期是否合法：官方要求 `yyyy-MM-dd`。
///
/// SDK 不引入时间库（见 [`now_unix_secs`] 的取舍），这里只做**形状**校验：
/// 具体日期（以及「只能取 T-1、三个月内」这条业务规则）由调用方负责。
pub(crate) fn is_bill_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

/// 从账单下载地址里取出**参与签名的 `path?query`**，并顺手挡掉两类地址。
///
/// `download_url` 来自「申请交易账单」的应答（该应答本身验签通过），但地址会被原样请求，
/// 因此这里仍需两条约束：
/// * **必须在 `/v3/` 之下** —— 官方给的地址就是 `/v3/billdownload/file?token=…`；少了这条，
///   一旦调用方在「关闭验签」模式（本地 mock）下把任意地址透传进来，本方法就等于一个
///   「给任意 URL 签名」的工具；
/// * **默认只接受 https** —— 只有调用方显式把客户端指向明文网关（`base_url` 是 `http://`，
///   本地联调）时才放行 http。
pub(crate) fn bill_download_path_and_query(
    url: &str,
    allow_plain_http: bool,
) -> Result<&str, PayError> {
    let rest = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if allow_plain_http {
        url.strip_prefix("http://").ok_or_else(|| {
            PayError::VerifyError(format!("账单下载地址必须是 http(s) 绝对地址: {url}"))
        })?
    } else {
        return Err(PayError::VerifyError(format!(
            "账单下载地址必须是 https 绝对地址: {url}"
        )));
    };
    let Some((_host, path_and_query)) = rest.split_once('/') else {
        return Err(PayError::VerifyError(format!(
            "账单下载地址缺少路径: {url}"
        )));
    };
    let path_and_query = &rest[rest.len() - path_and_query.len() - 1..];
    if !path_and_query.starts_with("/v3/") {
        return Err(PayError::VerifyError(format!(
            "账单下载地址必须在 /v3/ 之下: {url}"
        )));
    }
    Ok(path_and_query)
}

#[cfg(test)]
mod tests {
    use super::random_trade_no;

    #[test]
    fn random_trade_no_is_32_ascii_hex_chars() {
        let trade_no = random_trade_no();
        assert_eq!(trade_no.len(), 32);
        assert!(
            trade_no.chars().all(|c| c.is_ascii_hexdigit()),
            "订单号应为 ASCII 十六进制串，实际为 {trade_no}"
        );
    }

    #[test]
    fn random_trade_no_differs_across_calls() {
        assert_ne!(random_trade_no(), random_trade_no(), "两次调用应不同");
    }

    #[test]
    fn bill_date_shape_is_validated() {
        assert!(super::is_bill_date("2026-09-28"));
        for bad in ["2026-9-28", "20260928", "2026-09-2x", "", "2026/09/28"] {
            assert!(!super::is_bill_date(bad), "{bad} 不该通过");
        }
    }

    #[test]
    fn bill_download_url_must_be_v3_and_https() {
        assert_eq!(
            super::bill_download_path_and_query(
                "https://api.mch.weixin.qq.com/v3/billdownload/file?token=abc",
                false
            )
            .expect("官方地址应当被接受"),
            "/v3/billdownload/file?token=abc"
        );
        // 明文 http:只有调用方把客户端指向明文网关(本地联调)时才放行
        let plain = "http://127.0.0.1:9817/v3/billdownload/file?token=abc";
        assert!(super::bill_download_path_and_query(plain, false).is_err());
        assert_eq!(
            super::bill_download_path_and_query(plain, true).expect("联调网关应当可用"),
            "/v3/billdownload/file?token=abc"
        );
        // 非 /v3/ 之下的地址一律拒绝(否则本方法成了「给任意 URL 签名」的工具)
        for bad in [
            "https://evil.example.com/steal",
            "https://api.mch.weixin.qq.com/",
            "https://api.mch.weixin.qq.com",
            "not-a-url",
        ] {
            assert!(
                super::bill_download_path_and_query(bad, false).is_err(),
                "{bad} 不该被接受"
            );
        }
    }
}
