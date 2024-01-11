use serde_json::{Map, Value};
use tracing::debug;
use crate::error::PayError;
use crate::model::{H5Params, JsapiParams, NativeParams};
use crate::pay::WechatPay;
use crate::request::HttpMethod;
use crate::response::{H5Response, JsapiResponse, NativeResponse};

impl WechatPay {
    pub fn h5_pay(&self, params: H5Params) -> Result<H5Response, PayError> {
        let url = "/v3/pay/transactions/h5";
        let method = HttpMethod::POST;
        let json_str = serde_json::to_string(&params)?;
        debug!("h5_pay json_str: {}", json_str);
        let mut map: Map<String, Value> = serde_json::from_str(&json_str)?;
        map.insert("appid".to_owned(), self.appid().into());
        map.insert("mchid".to_owned(), self.mch_id().into());
        map.insert("notify_url".to_owned(), self.notify_url().into());

        let body = serde_json::to_string(&map)?;
        let headers = self.build_header(
            method,
            url,
            body.as_str(),
        )?;

        let client = reqwest::blocking::Client::new();
        let url = format!("{}{}", self.base_url(), url);
        debug!("url: {}", url);
        debug!("body: {}",body);
        client.post(url)
            .headers(headers)
            .body(body)
            .send()?
            .json::<H5Response>()
            .map(Ok)?
    }
    pub fn native_pay(&self, params: NativeParams) -> Result<NativeResponse, PayError> {
        let url = "/v3/pay/transactions/native";
        let method = HttpMethod::POST;
        let json_str = serde_json::to_string(&params)?;
        debug!("native_pay json_str: {}", json_str);
        let mut map: Map<String, Value> = serde_json::from_str(&json_str)?;
        map.insert("appid".to_owned(), self.appid().into());
        map.insert("mchid".to_owned(), self.mch_id().into());
        map.insert("notify_url".to_owned(), self.notify_url().into());

        let body = serde_json::to_string(&map)?;
        let headers = self.build_header(
            method,
            url,
            body.as_str(),
        )?;

        let client = reqwest::blocking::Client::new();
        let url = format!("{}{}", self.base_url(), url);
        debug!("url: {}", url);
        debug!("body: {}",body);
        client.post(url)
            .headers(headers)
            .body(body)
            .send()?
            .json::<NativeResponse>()
            .map(Ok)?
    }

    pub fn jsapi_pay(&self, params: JsapiParams) -> Result<JsapiResponse, PayError> {
        let url = "/v3/pay/transactions/jsapi";
        let method = HttpMethod::POST;
        let json_str = serde_json::to_string(&params)?;
        debug!("jsapi_pay json_str: {}", json_str);
        let mut map: Map<String, Value> = serde_json::from_str(&json_str)?;
        map.insert("appid".to_owned(), self.appid().into());
        map.insert("mchid".to_owned(), self.mch_id().into());
        map.insert("notify_url".to_owned(), self.notify_url().into());

        let body = serde_json::to_string(&map)?;
        let headers = self.build_header(
            method,
            url,
            body.as_str(),
        )?;

        let client = reqwest::blocking::Client::new();
        let url = format!("{}{}", self.base_url(), url);
        debug!("url: {}", url);
        debug!("body: {}",body);
        client.post(url)
            .headers(headers)
            .body(body)
            .send()?
            .json::<JsapiResponse>()
            .map(Ok)?
    }
}

#[cfg(test)]
mod tests {
    use dotenvy::dotenv;
    use tracing::debug;
    use crate::model::{H5Params, H5SceneInfo, JsapiParams, NativeParams, SceneInfo};
    use crate::pay::WechatPay;
    use crate::util;

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
        let private_key_path = "./apiclient_key.pem";
        let private_key = std::fs::read_to_string(private_key_path).unwrap();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay.native_pay(NativeParams::new(
            "测试支付1分",
            "1243243",
            1.into(),
        )).expect("native_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    pub fn test_jsapi_pay() {
        init_log();
        dotenv().ok();
        let private_key_path = "./apiclient_key.pem";
        let private_key = std::fs::read_to_string(private_key_path).unwrap();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay.jsapi_pay(JsapiParams::new(
            "测试支付1分",
            "1243243",
            1.into(),
            "open_id".into()
        )).expect("jsapi_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    pub fn test_h5_pay() {
        init_log();
        dotenv().ok();
        let private_key_path = "./apiclient_key.pem";
        let private_key = std::fs::read_to_string(private_key_path).unwrap();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay.h5_pay(H5Params::new(
            "测试支付1分",
            util::random_trade_no().as_str(),
            1.into(),
            H5SceneInfo::new("183.6.105.141", "ipa软件下载", "https://ipadump.com"),
        )).expect("h5_pay error");
        debug!("body: {:?}", body);
    }
}