use base64::Engine;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, USER_AGENT};
use rsa::pkcs8::DecodePrivateKey;
use sha2::Digest;
use tracing::debug;
use crate::request::HttpMethod;
use crate::sign;
use uuid::Uuid;
use crate::error::PayError;
use crate::response::SignData;

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
        Uuid::new_v4()
            .to_string()
            .replace("-", "")
            .to_uppercase()
    }

    fn mut_sign_data<S>(&self, prefix: S, prepay_id: S) -> SignData
        where S: AsRef<str>
    {
        let app_id = self.appid();
        let now_time = self.now_timestamp();
        let nonce_str = self.nonce_str();
        let ext_str = format!("{prefix}{prepay_id}", prefix = prefix.as_ref(), prepay_id = prepay_id.as_ref());
        let signed_str = self.rsa_sign(
            format!(
                "{app_id}\n{now_time}\n{nonce_str}\n{ext_str}\n"
            )
        );
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
        let private_key = std::env::var("WECHAT_PRIVATE_KEY").expect("WECHAT_PRIVATE_KEY not found");
        let serial_no = std::env::var("WECHAT_SERIAL_NO").expect("WECHAT_SERIAL_NO not found");
        let v3_key = std::env::var("WECHAT_V3_KEY").expect("WECHAT_V3_KEY not found");
        let notify_url = std::env::var("WECHAT_NOTIFY_URL").expect("WECHAT_NOTIFY_URL not found");
        let private_key = std::fs::read_to_string(private_key).expect("read private key error");
        Self::new(appid, mch_id, private_key, serial_no, v3_key, notify_url)
    }


    pub(crate) fn build_header(&self,
                               method: HttpMethod,
                               url: impl AsRef<str>,
                               body: impl AsRef<str>,
    ) -> Result<HeaderMap, PayError> {
        let method = method.to_string();
        let url = url.as_ref();
        let body = body.as_ref();
        let timestamp = chrono::Local::now().timestamp();
        let serial_no = self.serial_no.to_string();
        let nonce_str = Uuid::new_v4()
            .to_string()
            .replace("-", "")
            .to_uppercase();
        let message = format!(
            "{}\n{}\n{}\n{}\n{}\n",
            method,
            url,
            timestamp,
            nonce_str,
            body,
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
    use tracing::debug;
    use uuid::Uuid;
    use crate::pay::{WechatPay, WechatPayTrait};

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
        let wechat_pay = WechatPay::new(
            "",
            "",
            private_key.as_ref(),
            "",
            "",
            "",
        );
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
}
