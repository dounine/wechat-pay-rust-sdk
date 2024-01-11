use serde_json::{Map, Value};
use tracing::debug;
use crate::error::PayError;
use crate::model::NativeConfig;
use crate::pay::WechatPay;
use crate::request::HttpMethod;
use crate::response::NativeResponse;

#[cfg(feature = "async")]
impl WechatPay {
    #[cfg(feature = "async")]
    pub async fn native_pay(&self, body: NativeConfig) -> Result<NativeResponse, PayError> {
        let url = "/v3/pay/transactions/native";
        let method = HttpMethod::POST;
        let json_str = serde_json::to_string(&body)?;
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

        let client = reqwest::Client::new();
        let url = format!("{}{}", self.base_url(), url);
        debug!("url: {}", url);
        debug!("body: {}",body);
        client.post(url)
            .headers(headers)
            .body(body)
            .send()
            .await?
            .json::<NativeResponse>()
            .await
            .map(Ok)?
    }
}

#[cfg(test)]
mod tests {
    use dotenvy::dotenv;
    use tracing::debug;
    use crate::model::NativeConfig;
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
        let body = wechat_pay.native_pay(NativeConfig::new(
            "测试支付1分",
            "1243243",
            1.into(),
        )).await.expect("pay fail");
        debug!("body: {:?}", body);
    }
}