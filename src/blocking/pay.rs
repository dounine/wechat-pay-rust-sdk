use crate::debug;
use crate::error::PayError;
use crate::model::{
    AppParams, H5Params, JsapiParams, MicroParams, NativeParams, ParamsTrait, RefundsParams,
};
use crate::pay::{WechatPay, WechatPayTrait};
use crate::request::HttpMethod;
use crate::response::{
    AppResponse, CertificateResponse, H5Response, JsapiResponse, MicroResponse, NativeResponse,
    RefundsResponse, ResponseTrait, WeChatResponse,
};
use reqwest::header::{HeaderMap, REFERER};
use serde_json::{Map, Value};

impl WechatPay {
    pub fn pay<P: ParamsTrait, R: ResponseTrait>(
        &self,
        method: HttpMethod,
        url: &str,
        json: P,
    ) -> Result<R, PayError> {
        let json_str = json.to_json();
        debug!("json_str: {}", json_str);
        let mut map: Map<String, Value> = serde_json::from_str(&json_str)?;
        map.insert("appid".to_owned(), self.appid().into());
        map.insert("mchid".to_owned(), self.mch_id().into());
        map.insert("notify_url".to_owned(), self.notify_url().into());
        let body = serde_json::to_string(&map)?;
        let headers = self.build_header(method.clone(), url, body.as_str())?;
        let client = reqwest::blocking::Client::new();
        let url = format!("{}{}", self.base_url(), url);
        debug!("url: {} body: {}", url, body);
        let builder = match method {
            HttpMethod::GET => client.get(url),
            HttpMethod::POST => client.post(url),
            HttpMethod::PUT => client.put(url),
            HttpMethod::DELETE => client.delete(url),
            HttpMethod::PATCH => client.patch(url),
        };

        builder
            .headers(headers)
            .body(body)
            .send()?
            .json::<R>()
            .map(Ok)?
    }

    pub fn get_pay<R: ResponseTrait>(&self, url: &str) -> Result<R, PayError> {
        let body = "";
        let headers = self.build_header(HttpMethod::GET, url, body)?;
        let client = reqwest::blocking::Client::new();
        let url = format!("{}{}", self.base_url(), url);
        debug!("url: {} body: {}", url, body);
        client
            .get(url)
            .headers(headers)
            .body(body)
            .send()?
            .json::<R>()
            .map(Ok)?
    }

    pub fn h5_pay(&self, params: H5Params) -> Result<H5Response, PayError> {
        let url = "/v3/pay/transactions/h5";
        self.pay(HttpMethod::POST, url, params)
    }
    pub fn native_pay(&self, params: NativeParams) -> Result<NativeResponse, PayError> {
        let url = "/v3/pay/transactions/native";
        self.pay(HttpMethod::POST, url, params)
    }

    pub fn app_pay(&self, params: AppParams) -> Result<AppResponse, PayError> {
        let url = "/v3/pay/transactions/app";
        self.pay(HttpMethod::POST, url, params)
            .map(|mut result: AppResponse| {
                if let Some(prepay_id) = &result.prepay_id {
                    result.sign_data = Some(self.mut_sign_data("", prepay_id));
                }
                result
            })
    }

    pub fn micro_pay(&self, params: MicroParams) -> Result<MicroResponse, PayError> {
        let url = "/v3/pay/transactions/jsapi";
        self.pay(HttpMethod::POST, url, params)
            .map(|mut result: MicroResponse| {
                if let Some(prepay_id) = &result.prepay_id {
                    result.sign_data = Some(self.mut_sign_data("prepay_id=", prepay_id));
                }
                result
            })
    }

    /// jsapi支付
    /// ```rust
    /// use wechat_pay_rust_sdk::model::JsapiParams;
    /// use wechat_pay_rust_sdk::pay::WechatPay;
    /// let wechat_pay = WechatPay::from_env();
    ///
    /// let body = wechat_pay.jsapi_pay(JsapiParams::new(
    /// "测试支付1分",
    /// "1243243",
    /// 1.into(),
    /// "open_id".into(),
    /// )).expect("jsapi_pay error");
    /// println!("body: {:?}", body);
    /// ```
    pub fn jsapi_pay(&self, params: JsapiParams) -> Result<JsapiResponse, PayError> {
        let url = "/v3/pay/transactions/jsapi";
        self.pay(HttpMethod::POST, url, params)
            .map(|mut result: JsapiResponse| {
                if let Some(prepay_id) = &result.prepay_id {
                    result.sign_data = Some(self.mut_sign_data("prepay_id=", prepay_id));
                }
                result
            })
    }
    pub fn get_weixin<S>(&self, h5_url: S, referer: S) -> Result<Option<String>, PayError>
    where
        S: AsRef<str>,
    {
        let client = reqwest::blocking::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert(REFERER, referer.as_ref().parse().unwrap());
        let body = client.get(h5_url.as_ref()).headers(headers).send()?;
        let text = body.text()?;
        text.split("\n")
            .find(|line| line.contains("weixin://"))
            .map(|line| {
                line.split(r#"""#)
                    .find(|line| line.contains("weixin://"))
                    .map(|line| line.to_string())
            })
            .ok_or_else(|| PayError::WeixinNotFound)
    }
    pub fn certificates(&self) -> Result<CertificateResponse, PayError> {
        let url = "/v3/certificates";
        self.get_pay(url)
    }

    pub fn refunds(
        &self,
        params: RefundsParams,
    ) -> Result<WeChatResponse<RefundsResponse>, PayError> {
        let url = "/v3/refund/domestic/refunds";
        let body = params.to_json();
        let headers = self.build_header(HttpMethod::POST, url, body.as_str())?;
        let client = reqwest::blocking::Client::new();
        let url = format!("{}{}", self.base_url(), url);
        debug!("url: {} body: {}", url, body);
        let builder = client.post(url);

        builder
            .headers(headers)
            .body(body)
            .send()?
            .json::<WeChatResponse<RefundsResponse>>()
            .map(Ok)?
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{
        AppParams, H5Params, H5SceneInfo, JsapiParams, MicroParams, NativeParams, RefundsParams,
    };
    use crate::pay::{PayNotifyTrait, WechatPay};
    use crate::response::Certificate;
    use crate::util;
    use dotenvy::dotenv;
    use std::io::Write;
    use tracing::debug;

    #[inline]
    fn init_log() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_line_number(true)
            .init();
    }

    #[test]
    pub fn test_native_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .native_pay(NativeParams::new("测试支付1分", "1243243", 1.into()))
            .expect("native_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    pub fn test_jsapi_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .jsapi_pay(JsapiParams::new(
                "测试支付1分",
                "1243243",
                1.into(),
                "open_id".into(),
            ))
            .expect("jsapi_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    pub fn test_micro_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .micro_pay(MicroParams::new(
                "测试支付1分",
                "1243243",
                1.into(),
                "open_id".into(),
            ))
            .expect("micro_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    pub fn test_app_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .app_pay(AppParams::new("测试支付1分", "1243243", 1.into()))
            .expect("app_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    pub fn test_str() {
        let str = r#" deeplink : "weixin://wap/pay?prepayid%3Dwx122129234529163c948432e26bc0030000&package=4206921243&noncestr=1705066163&sign=788bc4a9f8f44c6f708aff38c4b48a85""#;
        let _strs = str.split(r#"""#).find(|line| line.contains("weixin://"));
    }

    #[test]
    pub fn test_h5_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .h5_pay(H5Params::new(
                "测试支付1分",
                util::random_trade_no().as_str(),
                1.into(),
                H5SceneInfo::new("183.6.105.141", "ipa软件下载", "https://mydomain.com"),
            ))
            .expect("h5_pay error");
        let weixin_url = wechat_pay
            .get_weixin(body.h5_url.unwrap().as_str(), "https://mydomain.com")
            .unwrap();
        debug!("weixin_url: {}", weixin_url.unwrap());
    }

    #[test]
    pub fn test_certificates() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let response = wechat_pay.certificates().expect("certificates error");
        let data = response.data.unwrap().first().unwrap().clone();
        let ciphertext = data.encrypt_certificate.ciphertext;
        let nonce = data.encrypt_certificate.nonce;
        let associated_data = data.encrypt_certificate.associated_data;
        let data = wechat_pay
            .decrypt_bytes(ciphertext, nonce, associated_data)
            .unwrap();
        let pub_key = util::x509_to_pem(data.as_slice()).unwrap();
        let mut pub_key_file = std::fs::File::create("pubkey.pem").unwrap();
        pub_key_file.write_all(pub_key.as_bytes()).unwrap();

        let (pub_key_valid, expire_timestamp) = util::x509_is_valid(data.as_slice()).unwrap();
        debug!(
            "pub key valid:{} expire_timestamp:{}",
            pub_key_valid, expire_timestamp
        ); //证书是否可用,过期时间
        debug!("pub key: {}", pub_key);
    }

    #[test]
    pub fn test_decode_certificates() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let response = wechat_pay.certificates().expect("certificates error");
        let data: Certificate = response.data.unwrap()[0].clone();
        let ciphertext = data.encrypt_certificate.ciphertext;
        let nonce = data.encrypt_certificate.nonce;
        let associated_data = data.encrypt_certificate.associated_data;
        let data = wechat_pay
            .decrypt_bytes(ciphertext, nonce, associated_data)
            .unwrap();
        debug!("data: {}", String::from_utf8_lossy(data.as_ref()));
    }

    #[test]
    pub fn test_blocking_refunds() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();

        let req = RefundsParams::new("123456", 1, 1, None, Some("123456"));

        let body = wechat_pay.refunds(req).expect("refunds fail");

        if body.is_success() {
            debug!("refunds success: {:?}", body.ok());
        } else {
            debug!("refunds error: {:?}", body.err());
        }
    }
}
