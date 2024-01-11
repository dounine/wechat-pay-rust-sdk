use serde_json::{Map, Value};
use tracing::debug;
use crate::error::PayError;
use crate::model::NativeParams;
use crate::model::JsapiParams;
use crate::model::ParamsTrait;
use crate::model::MicroParams;
use crate::model::AppParams;
use crate::model::H5Params;
use crate::pay::{WechatPay, WechatPayTrait};
use crate::request::HttpMethod;
use crate::response::NativeResponse;
use crate::response::ResponseTrait;
use crate::response::JsapiResponse;
use crate::response::MicroResponse;
use crate::response::AppResponse;
use crate::response::H5Response;

impl WechatPay {
    pub(crate) async fn pay<P: ParamsTrait, R: ResponseTrait>(&self, method: HttpMethod, url: &str, json: P) -> Result<R, PayError> {
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
        let client = reqwest::Client::new();
        let url = format!("{}{}", self.base_url(), url);
        debug!("url: {}", url);
        debug!("body: {}",body);
        client.post(url)
            .headers(headers)
            .body(body)
            .send()
            .await?
            .json::<R>()
            .await
            .map(Ok)?
    }

    pub async fn h5_pay(&self, params: H5Params) -> Result<H5Response, PayError> {
        let url = "/v3/pay/transactions/h5";
        self.pay(HttpMethod::POST, url, params).await
    }
    pub async fn app_pay(&self, params: AppParams) -> Result<AppResponse, PayError> {
        let url = "/v3/pay/transactions/app";
        self
            .pay(HttpMethod::POST, url, params)
            .await
            .map(|mut result: AppResponse| {
                if let Some(prepay_id) = &result.prepay_id {
                    result.sign_data = Some(self.mut_sign_data("", prepay_id));
                }
                result
            })
    }
    pub async fn jsapi_pay(&self, params: JsapiParams) -> Result<JsapiResponse, PayError> {
        let url = "/v3/pay/transactions/jsapi";
        self
            .pay(HttpMethod::POST, url, params)
            .await
            .map(|mut result: JsapiResponse| {
                if let Some(prepay_id) = &result.prepay_id {
                    result.sign_data = Some(self.mut_sign_data("", prepay_id));
                }
                result
            })
    }
    pub async fn micro_pay(&self, params: MicroParams) -> Result<MicroResponse, PayError> {
        let url = "/v3/pay/transactions/jsapi";
        self
            .pay(HttpMethod::POST, url, params)
            .await
            .map(|mut result: MicroResponse| {
                if let Some(prepay_id) = &result.prepay_id {
                    result.sign_data = Some(self.mut_sign_data("", prepay_id));
                }
                result
            })
    }
    pub async fn native_pay(&self, params: NativeParams) -> Result<NativeResponse, PayError> {
        let url = "/v3/pay/transactions/native";
        self.pay(HttpMethod::POST, url, params).await
    }
}

#[cfg(test)]
mod tests {
    use dotenvy::dotenv;
    use tracing::debug;
    use crate::model::NativeParams;
    use crate::pay::WechatPay;

    #[inline]
    fn init_log() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_line_number(true)
            .init();
    }

    #[tokio::test]
    pub async fn test_native_pay() {
        init_log();
        dotenv().ok();
        let private_key_path = "./apiclient_key.pem";
        let private_key = std::fs::read_to_string(private_key_path).unwrap();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay.native_pay(NativeParams::new(
            "测试支付1分",
            "1243243",
            1.into(),
        )).await.expect("pay fail");
        debug!("body: {:?}", body);
    }
}