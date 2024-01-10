# wechat-pay-rust-sdk
[![Latest Version](https://img.shields.io/crates/v/wechat-pay-rust-sdk.svg)](https://crates.io/crates/wechat-pay-rust-sdk)

微信支付 © Wechat Pay SDK Official(标准库)

# API文档
1. [native支付](#native支付)
2. [jsapi支付](#jsapi支付)
3. [app支付](#app支付)
4. [h5支付](#h5支付)
5. [小程序支付](#小程序支付)

## 使用指南
引入依赖
```toml
wechat-pay-rust-sdk = {version = "0.1.0"}
#开启异步 wechat-pay-rust-sdk = {version = "0.1.0", features = ["async"]}
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
```
输出
```rust
NativeResponse { code: None, message: None, code_url: Some("weixin://wxpay/bizpayurl?pr=yL2aIPzz") }
```

