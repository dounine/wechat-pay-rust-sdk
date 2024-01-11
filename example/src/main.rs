use wechat_pay_rust_sdk::model::{NativeParams};
use wechat_pay_rust_sdk::pay::WechatPay;

#[tokio::main]
async fn main() {
    println!("Hello, world!");
    dotenvy::dotenv().ok();
    let wechat_pay = WechatPay::from_env();
    let result = wechat_pay.native_pay(NativeParams::new(
        "测试支付1分",
        "124324342",
        1.into(),
    )).await.unwrap();
    println!("result: {:?}", result);
}
