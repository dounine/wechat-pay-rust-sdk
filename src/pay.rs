use base64::Engine;
use rsa::pkcs8::DecodePrivateKey;
use sha2::Digest;
use crate::sign;

#[derive(Debug)]
pub struct WechatPay {
    appid: String,
    mch_id: String,
    private_key: String,
    v3_key: Option<String>,
    notify_url: Option<String>,
}

unsafe impl Send for WechatPay {}

unsafe impl Sync for WechatPay {}

impl WechatPay {
    pub fn new<S: AsRef<str>>(
        appid: S,
        mch_id: S,
        private_key: S,
        v3_key: Option<S>,
        notify_url: Option<S>,
    ) -> Self {
        Self {
            appid: appid.as_ref().to_string(),
            mch_id: mch_id.as_ref().to_string(),
            private_key: private_key.as_ref().to_string(),
            v3_key: v3_key.map(|x| x.as_ref().to_string()),
            notify_url: notify_url.map(|x| x.as_ref().to_string()),
        }
    }

    pub fn rsa_sign(&self, content: impl AsRef<str>) -> String {
        let private_key = self.private_key.as_ref();
        sign::sha256_sign(private_key, content.as_ref())
    }

}


#[cfg(test)]
mod tests {
    use tracing::debug;
    use crate::pay::WechatPay;

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
            Some(""),
            None,
        );
        let sign_str = wechat_pay.rsa_sign("hello");
        debug!("sign_str: {}", sign_str);
    }
}
