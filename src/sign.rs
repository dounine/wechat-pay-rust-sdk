use crate::error::PayError;
use crate::util;
use rsa::pkcs8::DecodePrivateKey;
use rsa::sha2::Digest;
use rsa::{Pkcs1v15Sign, RsaPrivateKey};

/// 解析商户 API 私钥（**PKCS#8 PEM**）。
///
/// 与 [`sha256_sign`] 拆开是为了「解析一次、签名多次」：`from_pkcs8_pem` 会做密钥校验
/// 并预计算 CRT 参数，放在请求路径上既慢，又只能以 panic 收场（坏 PEM 是**配置错误**，
/// 应当作为 `PayError` 返回给调用方）。
pub(crate) fn parse_private_key(pem: &str) -> Result<RsaPrivateKey, PayError> {
    RsaPrivateKey::from_pkcs8_pem(pem)
        .map_err(|e| PayError::SignError(format!("商户私钥 PEM 解析失败: {e}")))
}

/// 用已解析的私钥做 RSA-SHA256（PKCS#1 v1.5）签名，返回 base64（STANDARD 字母表）。
pub(crate) fn sha256_sign(key: &RsaPrivateKey, content: &str) -> Result<String, PayError> {
    let hasher = rsa::sha2::Sha256::new().chain_update(content).finalize();
    let padding = Pkcs1v15Sign::new::<rsa::sha2::Sha256>();
    let signature = key
        .sign(padding, &hasher)
        .map_err(|e| PayError::SignError(format!("RSA 签名失败: {e}")))?;
    Ok(util::base64_encode(signature))
}
