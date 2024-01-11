# wechat-pay-rust-sdk
[![Latest Version](https://img.shields.io/crates/v/wechat-pay-rust-sdk.svg)](https://crates.io/crates/wechat-pay-rust-sdk)

微信支付 © Wechat Pay SDK Official(标准库)

# API文档
1. [native支付](#native支付)
2. [jsapi支付](#jsapi支付)
3. [app支付](#app支付)
4. [h5支付](#h5支付)
5. [小程序支付](#小程序支付)

# 使用指南
引入依赖
```toml
wechat-pay-rust-sdk = {version = "0.1.2", features = ["blocking"]}
#异步 wechat-pay-rust-sdk = {version = "0.1.2", features = ["async"]}
```

## native支付
```rust
use wechat_pay_rust_sdk::model::NativeConfig;
use wechat_pay_rust_sdk::pay::WechatPay;

let private_key_path = "./apiclient_key.pem";
let private_key = std::fs::read_to_string(private_key_path).unwrap();
let wechat_pay = WechatPay::new(
    "app_id",
    "mch_id",
    private_key.as_str(),
    "serial_no",
    "v3_key",
    "notifi_url",
);
let body = wechat_pay.native_pay(NativeConfig::new(
    "测试支付1分",
    "124324343",
    1.into(),
)).expect("native_pay error");
println!("body: {:?}", body);
```
输出
```rust
NativeResponse { 
    code: None, 
    message: None, 
    code_url: Some("weixin://wxpay/bizpayurl?pr=yL2aIPzz") 
}
```
## h5支付

```rust
use wechat_pay_rust_sdk::model::{H5Params, H5SceneInfo};
use wechat_pay_rust_sdk::pay::WechatPay;
use wechat_pay_rust_sdk::util;

let wechat_pay = WechatPay::from_env();
let body = wechat_pay.h5_pay(H5Params::new(
    "支付1分",
    util::random_trade_no().as_str(),
    1.into(),
    H5SceneInfo::new(
           "183.6.105.1", //填写客户端IP
           "我的网站",
           "https://mydomain.com",
   ),
)).expect("h5_pay error");
println!("body: {:?}", body);
```

输出
```
H5Response { 
    code: None, 
    message: None, 
    h5_url: Some("https://wx.tenpay.com/cgi-bin/mmpayweb-bin/checkmweb?prepay_id=wx11154002858116623fasdfasdf&package=760499411") 
}
```

## jsapi支付

```rust
use wechat_pay_rust_sdk::model::JsapiParams;
use wechat_pay_rust_sdk::pay::WechatPay;

let private_key_path = "./apiclient_key.pem";
let private_key = std::fs::read_to_string(private_key_path).unwrap();
let wechat_pay = WechatPay::from_env();
let body = wechat_pay.jsapi_pay(JsapiParams::new(
     "测试支付1分",
     "1243243",
     1.into(),
     "open_id".into()
     )).expect("jsapi_pay error");
println!("body: {:?}", body);
 ```
 输出
 ```rust
H5Response { 
    code: None, 
    message: None, 
    prepay_id: Some("wx201410272009395522657a690389285100") 
}
 ```
