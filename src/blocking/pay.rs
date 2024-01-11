use serde::Deserialize;
use serde_json::{Map, Value};
use tracing::debug;
use crate::error::PayError;
use crate::model::{AppParams, H5Params, JsapiParams, MicroParams, NativeParams, ParamsTrait};
use crate::pay::{WechatPay, WechatPayTrait};
use crate::request::HttpMethod;
use crate::response::{AppResponse, H5Response, JsapiResponse, MicroResponse, NativeResponse, ResponseTrait, SignData};

impl WechatPay {
    pub(crate) fn pay<P: ParamsTrait, R: ResponseTrait>(&self, method: HttpMethod, url: &str, json: P) -> Result<R, PayError> {
        let json_str = json.to_json();
        debug!("json_str: {}", &json_str);
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
        self
            .pay(HttpMethod::POST, url, params)
            .map(|mut result: AppResponse| {
                if let Some(prepay_id) = &result.prepay_id {
                    result.sign_data = Some(self.mut_sign_data("", prepay_id));
                }
                result
            })
    }

    pub fn micro_pay(&self, params: MicroParams) -> Result<MicroResponse, PayError> {
        let url = "/v3/pay/transactions/jsapi";
        self
            .pay(HttpMethod::POST, url, params)
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
        self
            .pay(HttpMethod::POST, url, params)
            .map(|mut result: JsapiResponse| {
                if let Some(prepay_id) = &result.prepay_id {
                    result.sign_data = Some(self.mut_sign_data("prepay_id=", prepay_id));
                }
                result
            })
    }
}

#[cfg(test)]
mod tests {
    use dotenvy::dotenv;
    use tracing::debug;
    use crate::model::{AppParams, H5Params, H5SceneInfo, JsapiParams, MicroParams, NativeParams, SceneInfo};
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
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay.jsapi_pay(JsapiParams::new(
            "测试支付1分",
            "1243243",
            1.into(),
            "open_id".into(),
        )).expect("jsapi_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    pub fn test_micro_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay.micro_pay(MicroParams::new(
            "测试支付1分",
            "1243243",
            1.into(),
            "open_id".into(),
        )).expect("micro_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    pub fn test_app_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay.app_pay(AppParams::new(
            "测试支付1分",
            "1243243",
            1.into(),
        )).expect("app_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    pub fn test_h5_pay() {
        init_log();
        dotenv().ok();
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