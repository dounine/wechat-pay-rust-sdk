use crate::error::PayError;
use crate::model::WechatPayDecodeData;
use crate::request::HttpMethod;
use crate::response::SignData;
use crate::{sign, util};
use aes_gcm::aead::{AeadMut, Payload};
use aes_gcm::{
    aead::{AeadCore, AeadInPlace, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use reqwest::header::{HeaderMap, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::sha2::{Digest, Sha256};
use rsa::signature::DigestVerifier;
use rsa::{Pkcs1v15Sign, RsaPublicKey};
use tracing::debug;
use uuid::Uuid;

#[derive(Debug)]
pub struct WechatPay {
    appid: String,
    mch_id: String,
    private_key: String,
    serial_no: String,
    v3_key: String,
    notify_url: String,
    base_url: String,
}

unsafe impl Send for WechatPay {}

unsafe impl Sync for WechatPay {}

pub trait PayNotifyTrait: WechatPayTrait {
    fn verify_signatrue<S>(
        &self,
        pub_key: &str,
        timestamp: S,
        nonce: S,
        signatrue: S,
        body: S,
    ) -> Result<(), PayError>
    where
        S: AsRef<str>,
    {
        let message = format!(
            "{}\n{}\n{}\n",
            timestamp.as_ref(),
            nonce.as_ref(),
            body.as_ref()
        );
        let pub_key = RsaPublicKey::from_public_key_pem(pub_key)
            .map_err(|e| PayError::VerifyError("public key parser error".to_string()))?;
        let hashed = Sha256::new().chain_update(message).finalize();
        let signatrue = util::base64_decode(signatrue.as_ref())?;
        let scheme = Pkcs1v15Sign::new::<Sha256>();
        pub_key
            .verify(scheme, &hashed, signatrue.as_slice())
            .map_err(|e| PayError::VerifyError(e.to_string()))
    }
    fn decrypt_paydata<S>(
        &self,
        ciphertext: S,
        nonce: S,
        associated_data: S,
    ) -> Result<WechatPayDecodeData, PayError>
    where
        S: AsRef<str>,
    {
        let plaintext = self.decrypt_bytes(ciphertext, nonce, associated_data)?;
        let data: WechatPayDecodeData = serde_json::from_slice(&plaintext)?;
        Ok(data)
    }
    fn decrypt_bytes<S>(
        &self,
        ciphertext: S,
        nonce: S,
        associated_data: S,
    ) -> Result<Vec<u8>, PayError>
    where
        S: AsRef<str>,
    {
        if nonce.as_ref().len() != 12 {
            return Err(PayError::DecryptError(
                "nonce length must be 12".to_string(),
            ));
        }
        let v3_key = self.v3_key();
        let ciphertext = util::base64_decode(ciphertext.as_ref())?;
        let aes_key = v3_key.as_str().as_bytes();
        let mut cipher = Aes256Gcm::new(aes_key.into());
        let payload = Payload {
            msg: &ciphertext.as_slice(),
            aad: &associated_data.as_ref().as_bytes(),
        };
        let plaintext = cipher
            .decrypt(nonce.as_ref().as_bytes().into(), payload)
            .map_err(|e| PayError::DecryptError(e.to_string()))?;
        Ok(plaintext)
    }
}

pub(crate) trait WechatPayTrait {
    fn appid(&self) -> String;
    fn mch_id(&self) -> String;
    fn private_key(&self) -> String;
    fn serial_no(&self) -> String;
    fn v3_key(&self) -> String;
    fn notify_url(&self) -> String;
    fn base_url(&self) -> String;
    fn rsa_sign(&self, content: impl AsRef<str>) -> String;
    fn now_timestamp(&self) -> String {
        chrono::Local::now().timestamp().to_string()
    }
    fn nonce_str(&self) -> String {
        Uuid::new_v4().to_string().replace("-", "").to_uppercase()
    }

    fn mut_sign_data<S>(&self, prefix: S, prepay_id: S) -> SignData
    where
        S: AsRef<str>,
    {
        let app_id = self.appid();
        let now_time = self.now_timestamp();
        let nonce_str = self.nonce_str();
        let ext_str = format!(
            "{prefix}{prepay_id}",
            prefix = prefix.as_ref(),
            prepay_id = prepay_id.as_ref()
        );
        let signed_str = self.rsa_sign(format!("{app_id}\n{now_time}\n{nonce_str}\n{ext_str}\n"));
        SignData {
            app_id,
            sign_type: "RSA".into(),
            package: ext_str,
            nonce_str,
            timestamp: now_time,
            pay_sign: signed_str,
        }
    }
}

impl PayNotifyTrait for WechatPay {}

impl WechatPayTrait for WechatPay {
    fn appid(&self) -> String {
        self.appid.clone()
    }
    fn mch_id(&self) -> String {
        self.mch_id.clone()
    }
    fn private_key(&self) -> String {
        self.private_key.clone()
    }
    fn serial_no(&self) -> String {
        self.serial_no.clone()
    }
    fn v3_key(&self) -> String {
        self.v3_key.clone()
    }
    fn notify_url(&self) -> String {
        self.notify_url.clone()
    }

    fn base_url(&self) -> String {
        self.base_url.clone()
    }

    fn rsa_sign(&self, content: impl AsRef<str>) -> String {
        let private_key = self.private_key.as_ref();
        sign::sha256_sign(private_key, content.as_ref())
    }
}

impl WechatPay {
    fn with_base_url(mut self, base_url: impl AsRef<str>) -> Self {
        self.base_url = base_url.as_ref().to_string();
        self
    }
    pub fn new<S: AsRef<str>>(
        appid: S,
        mch_id: S,
        private_key: S,
        serial_no: S,
        v3_key: S,
        notify_url: S,
    ) -> Self {
        Self {
            appid: appid.as_ref().to_string(),
            mch_id: mch_id.as_ref().to_string(),
            private_key: private_key.as_ref().to_string(),
            serial_no: serial_no.as_ref().to_string(),
            v3_key: v3_key.as_ref().to_string(),
            notify_url: notify_url.as_ref().to_string(),
            base_url: "https://api.mch.weixin.qq.com".to_string(),
        }
    }

    pub fn from_env() -> Self {
        let appid = std::env::var("WECHAT_APPID").expect("WECHAT_APPID not found");
        let mch_id = std::env::var("WECHAT_MCH_ID").expect("WECHAT_MCH_ID not found");
        let private_key =
            std::env::var("WECHAT_PRIVATE_KEY").expect("WECHAT_PRIVATE_KEY not found");
        let serial_no = std::env::var("WECHAT_SERIAL_NO").expect("WECHAT_SERIAL_NO not found");
        let v3_key = std::env::var("WECHAT_V3_KEY").expect("WECHAT_V3_KEY not found");
        let notify_url = std::env::var("WECHAT_NOTIFY_URL").expect("WECHAT_NOTIFY_URL not found");
        let private_key = std::fs::read_to_string(private_key).expect("read private key error");
        Self::new(appid, mch_id, private_key, serial_no, v3_key, notify_url)
    }

    pub(crate) fn build_header(
        &self,
        method: HttpMethod,
        url: impl AsRef<str>,
        body: impl AsRef<str>,
    ) -> Result<HeaderMap, PayError> {
        let method = method.to_string();
        let url = url.as_ref();
        let body = body.as_ref();
        let timestamp = chrono::Local::now().timestamp();
        let serial_no = self.serial_no.to_string();
        let nonce_str = Uuid::new_v4().to_string().replace("-", "").to_uppercase();
        let message = format!(
            "{}\n{}\n{}\n{}\n{}\n",
            method, url, timestamp, nonce_str, body,
        );
        debug!("rsa_sign message: {}", message);
        let signature = self.rsa_sign(message);
        let authorization = format!(
            "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",signature=\"{}\",timestamp=\"{}\",serial_no=\"{}\"",
            self.mch_id,
            nonce_str,
            signature,
            timestamp,
            serial_no,
        );
        debug!("authorization: {}", authorization);
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "application/json".parse().unwrap());
        let chrome_agent = "Mozilla/5.0 (Linux; Android 10; Redmi K30 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/86.0.4240.198 Mobile Safari/537.36";
        headers.insert(USER_AGENT, chrome_agent.parse().unwrap());
        headers.insert(AUTHORIZATION, authorization.parse().unwrap());
        headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
        Ok(headers)
    }
}

#[cfg(test)]
mod tests {
    use crate::error::PayError;
    use crate::model::WechatPayDecodeData;
    use crate::pay::{PayNotifyTrait, WechatPay, WechatPayTrait};
    use crate::util;
    use aes_gcm::aead::generic_array::GenericArray;
    use aes_gcm::aead::{Aead, Payload};
    use aes_gcm::{Aes256Gcm, KeyInit};
    use dotenvy::dotenv;
    use rsa::pkcs8::DecodePublicKey;
    use rsa::sha2::{Digest, Sha256, Sha512};
    use rsa::signature::DigestVerifier;
    use rsa::{Pkcs1v15Sign, RsaPublicKey};
    use serde::__private::from_utf8_lossy;
    use std::io;
    use tracing::{debug, error};
    use uuid::Uuid;

    #[inline]
    fn init_log() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_line_number(true)
            .init();
    }

    #[test]
    fn test_rsa_sign() {
        init_log();
        let private_key_path = "./apiclient_key.pem";
        let private_key = std::fs::read_to_string(private_key_path).unwrap();
        let wechat_pay = WechatPay::new("", "", private_key.as_ref(), "", "", "");
        let sign_str = wechat_pay.rsa_sign("hello");
        debug!("sign_str: {}", sign_str);
    }

    #[test]
    fn test_uuid_v4() {
        init_log();
        let timestamp = chrono::Local::now().timestamp();
        let uuid = Uuid::new_v4().to_string().replace("-", "");
        debug!("uuid: {}", uuid);
        debug!("timestamp: {}", timestamp);
    }

    /// 支付回调参数解密
    #[test]
    fn test_decrypt_paydata() {
        init_log();
        dotenv().ok();
        let associated_data = "transaction";
        let nonce = "gZiqzlfayUu2";
        let ciphertext = "pCidqdiS5IIj5f9Pw9j69zuzu8l8IxcPCkfsTBKzna4gqZztNAqTMUY/Ai0rtj8qhaX0naYZF3a2lRid/ofK/83MNv+Neb5+w/0+UOO9nLNJvIFy3oFeMf2PTbp6tgDE35T5AoP9iKQ+1VkXTiUdRxzFoRx6/LfBzHmeuVEDHKScRqjrf6NdxuDDD0ciCQaiHmb18Y0BRZdfNxWTAC83Rar5yTX2NNZPBtGdFDG3yAK2I3Vp7ZKLeMa92ecExNGwHrdJ+HxWw66IIdwVqJLlNmTG0c5zUpSc8yovnaJi1Wv/TC7Tm5NzcwdHsdRE110tIWFbvNmIzIIb+3P33JFWmaXXb1VVDC43DqtlplttYwL6H3kU0ABgHMMbccTwYmP4cSY8BCAL01754nqipxWogEC/la9iQiw85+rLRo/Ny9k3mp8n35D6bDNtS1LiaslbLM92ZbfKeglTg54F/R1l5xWolAVpx8iTz8Oc+XJClXdWr8j5poyh8zK2/RrXPRfr+8s2/oGeGvdaqJbN/LviYcCMDbXU9pKDScWlSi4akxfJu0EatPDvFEbn5DYRQnn5v6wCeesYkEL+wiFCAIs=";
        let wechat_pay = WechatPay::from_env();
        let data = wechat_pay
            .decrypt_paydata(ciphertext, nonce, associated_data)
            .unwrap();
        debug!("data: {:#?}", data);
    }

    /// 把微信支付平台证书序列号转换成16进制字符串
    /// ```text
    /// -----BEGIN CERTIFICATE-----
    /// xxxxasdfaskd内容fjalskdfjasdf
    /// -----END CERTIFICATE-----
    /// ```
    #[test]
    fn test_decrypt_certificates() {
        init_log();
        dotenv().ok();
        let associated_data = "certificate";
        let nonce = "bf003ed52d71";
        let ciphertext = "HE9tL+x8Mag2627GPRXBmaQxZPVhAm3f2UoxgHvVW+m6eN0vq6ggFf4UsaQ8ifeGKwjhj9M6ObREHNogrT5JlEDV4Mfg8pAcLNvKUnbQZeBFKtp8kXPy0KFGhfSWcMZ4+HyfAUkgqLpdRUuNpG3gSJptnfrbJktdtYifkDOcei+1ncq8x+aWCXkFw8l9xBSN8MVSf66TiKyuPD/QCKYbD92HHfmDHk2b8J+BKyISDlQTlKjpb9M01EnuPsIXi4Rww1YzZP8XDruRTFxxDxGmk74tu1cjGXzTIcNmFu85eHbLWENvoLttl/4cKLJ8w49PuCyrREACz1YeAOscEHsqYHaQ0VE2N/8J0wCBuQa+AVD6ra59lmCxRJOVfgQNTShxonA6uCfaPGtyg+5qlwYTESnSdIy2ODlXaOfzMT5N7/actJsEf2C7RJXTPWn79M5slVfE3gOh9aR3mJaEMFM9KZywqv4OT0OI9mpLqRLAV/QCkJ0q2SKCcZyIuLa+VAPVS5Rh2feQkP40iizvVPN68YMOAmVgMBYLaxehGnetT2UylTlqsov35hsbfKOEN5ArSr7y4xoTjW5BV4S0s2IDzHTHWQpMlTxJ59/sgMoq8+m8vezJ0W4AZubwG/iSQ+/tzv1CXAVUgMO8ZqEALpGiROVq+9hdD5a0UB6cGuOTw9OiQHLSn1M4zV2jWDLQSZ+Q8KFhTpMibnvdLFmC09k26K75VcACsNPSa9U+nvP9sp3H7a39Y9BXjIz8/Yd707Y8h76MpEWLsVTn7FvRWwaCi4vxZN/LMRh9KTLNffQcb5amoDYKVSr5BTshdM7EosNwQmGenNnAFlNE/mabXSIz+FC3gMlDbxVvaoB5vOLB/YHrqfoLMEtYGm2HGqjppLmkbNM/R/6NIDFe+jaXZPWh9Bt8F4blihJnbEsZlC/w0/2OylTUsjRipG443XhOLEZJgD54KOnQdpqDah+AW2tPq5V9528ePK5xJzZ33MB3kjgnmljaF5cVbgUcCp5e8N+zvFVoyltsYMNNrtOan0Zfpsj9hNPnUVKLEnsjGXyfpBazRKoOOrPK5MImLUt/JblT+PFZ1oSrQE1IRRfF88yaUYwY2qk3pTrqBY676hOIUesWwuN4CSm28lLu/VarJaY0iLKuoGF0eikGFnAae1BIuFxvDUc3C+vC4GXUFn9jr3PZQcGJuI3MbEk8xGFWcU8UBU2wWhRu5lIgFSX5krbe+FWmRSjl6Rc3s7HZi5Xa8RiRuN1bOcnhVYkNYXy1fg7lXoopWJJPgtMO/+DDTNGQe8G0UgQxy+OK0urlhtzGQjVhF838i0heG4JV+OWUKj/Qvoj/dxVVfIbfroupkg8GvMmn0Cq+nAuo0D0fvhQshDmRsL/a006piEiLthruMn/gymk8cccMVvzn+DxYfYH/WX2UKZ235hPynVLUo8FBBedVTQK3JuJHCT4Kz0lL28KRLpE+lW3/bzG9s0Bly7/h1BF5Xunv4TWYhMFseWGMRIiKR7HxMSXbD4Q1PQJrZt/DtP3JbPURfc6fuYPIb7iuka0kDkPGSCV2uCpzjVHZXYQWrDhFv7LWi4SUw+2mCZLsLR6kesexb5bBOMVRxnA/5WmYVp73WzXar28CW3l0WCccGL/EdVdhrx09RoW5GSy9zcjbyGhwZQuzZECbf/wCpd26YlMTzFP0bqfL/QJ4g32TX8XweyhTPRI7FX1Bg8x85GJYG/bvecR40lDj4A0WKGnVbic3e7LQpDi/BP9adDBxx3Nl0iCN9BUlMx6ypNmrQWHwQXgmPwapyByK0FHjmf0u7hExZ7+xMa9/DPo2YPJdAY6zuHlNUIXLEVa9/VrclsYbyGkeohFGsMgY5MIA0ZF5FFxEOQ31gtNgQiGIVywSGJS8L+qB3tDc07O8hMxCY9wKPP2ua0MkkKQ7O6cr3W1DxNsd9NCbENDW4zNHzT+4pafS1TFaEy0nHI/wIQEyJlXD";
        let wechat_pay = WechatPay::from_env();
        let data = wechat_pay
            .decrypt_bytes(ciphertext, nonce, associated_data)
            .unwrap();
        // debug!("data: {}", String::from_utf8(data).unwrap());
    }

    #[test]
    fn test_verify_sign() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let pub_key = std::fs::read_to_string("pub.pem").unwrap();
        let wechatpay_signatrue = "mFgmwXAKL3YJj34b7f+cUG3vkW09TiXU4lOSzCbvWFtvyLTb5WiyfAiVXZmMB17Qh9gDVkqboO97zfIYfv+AVdxj3GQljWlW+vE1Ujn2uxiFld6bWwz8Znk+833ruzZ8mAIaqLEjI/HKuVPdTj4LFzh/EO+gEMR6WDXr+7cZV7D3qUTXuO26fHLe0PmleDziG8SPgYjihK1ztF3Os0NhvL5tQMM8LKDOMzO3kxSr/TqTBtsB/OnuP2mH8yaSUeYeTpGStYvSw8KVi+gk6VnrlkVmdFh3DDXY60GCzCZ8zPl12RmzZbBRSK8ocVrzs4tuqRa5Euk3cDIA6qHqS8hyBQ==";
        let body = r#"{"id":"29a61973-babf-599a-966d-6bcdcf17360c","create_time":"2024-01-12T21:39:44+08:00","resource_type":"encrypt-resource","event_type":"TRANSACTION.SUCCESS","summary":"支付成功","resource":{"original_type":"transaction","algorithm":"AEAD_AES_256_GCM","ciphertext":"5ZfDK+LRJakAkC7kdHKRzCu5WZ0JFC2qSwP4InWNFeUnY0uaOnzfCjiqhDTFYyP4ywxuLxPUOiVI3WT6CcU0NNqbadTQ5XzjVuKLxYSnOYCFULltIrfsT/mUF4VW+xBMgSgG4+ZdzhRXVr+AzihDKFjw2p1iCtLYz9emgToctygNBtV6JDEI2BnCoiEM7qyIU1ALv5IsufQHDQqzjYXd16OD3i6O8UeSE2GOd4ifmQrAKGKalwWPECI73/qTFoAcLcgbhhn1TeSEaHoF7xceDmkL9AGlC21pBwYWoibTgqdlDJiz3IctrCzH6PPXD8XcApEj4A3ByyPjaNs6HxaJGzEHYGUkyM2/b7SzZIzqlBmNRZYFvBC0BOwoktyxrIhg3bKSbYtDYt1+8lMaYIJW6Dgq9GjG6pxAVrYULt8sk8cKZ+OrK9iXHZI11pYyK9YwWJLXbs6GyjMdDxhaGilF9csK8ZSsKzUjvlcLCjboCFX6nuHvCbswchYchQhTeitKDKG3/q+4snY183dBA6rXBHKQduqc1vXRR6odMcU1Evvy5mKnDTDELlI6mqvBtJ10XNED5O43ga5ZAODxYoU=","associated_data":"transaction","nonce":"uaGeNnBYNjl7"}}"#;
        let wechatpay_timestamp = "1705066785";
        let wechatpay_nonce = "Jh9oPZelCJIQeQ47kz4stzvDKpLEUhCX";
        wechat_pay
            .verify_signatrue(
                pub_key.as_str(),
                wechatpay_timestamp,
                wechatpay_nonce,
                wechatpay_signatrue,
                body,
            )
            .unwrap();
    }

    #[test]
    fn test_pay_verify_sign() {
        use rsa::{
            pkcs1v15::{Signature, VerifyingKey},
            signature::Verifier,
        };

        let signature = std::fs::read("signature.txt").unwrap();
        let message = std::fs::read("message.txt").unwrap();
        let pub_key = RsaPublicKey::read_public_key_pem_file("pub.pem").unwrap();

        /// 方法1：可行
        let hashed = rsa::sha2::Sha256::new()
            .chain_update(message.as_slice())
            .finalize();
        let scheme = Pkcs1v15Sign::new::<Sha256>();
        pub_key
            .verify(scheme, &hashed, signature.as_slice())
            .expect("签名验证失败");

        // 方法2：错误
        // let signature = Signature::try_from(signature.as_slice()).expect("签名解析失败");
        // let verifying_key: VerifyingKey<Sha256> = VerifyingKey::from(pub_key.clone());
        // verifying_key
        //     .verify(message.as_slice(), &signature)
        //     .expect("签名验证失败")
    }
}
