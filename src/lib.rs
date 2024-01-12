/*! # wechat-pay-rust-sdk

[![Latest Version](https://img.shields.io/crates/v/wechat-pay-rust-sdk.svg)](https://crates.io/crates/wechat-pay-rust-sdk)

微信支付 © Wechat Pay SDK Official (标准库)

[![QQ群](https://img.shields.io/badge/QQ%E7%BE%A4-799168925-blue)](http://qm.qq.com/cgi-bin/qm/qr?_wv=1027&k=dLoye8pBcO60zGzqLjGO0l-GgMIaf6wQ&authKey=LfxBdZ5A%2F9eWJbKpzTcuWPjmQu5UdIJ3TVTpqRAQYkCID50WLkYoIXcGxGKzupG3&noverify=0&group_code=799168925)

# API文档
1. [native支付](#native支付)
2. [jsapi支付](#jsapi支付)
3. [app支付](#app支付)
4. [h5支付](#h5支付)
5. [小程序支付](#小程序支付)
6. [支付回调解密](#支付回调解密)

# 使用指南
引入依赖
```toml
wechat-pay-rust-sdk = {version = "0.1.2", features = ["blocking"]}
#异步 wechat-pay-rust-sdk = {version = "0.1.2", features = ["async"]}
```

## native支付

```rust
use wechat_pay_rust_sdk::model::NativeParams;
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
let body = wechat_pay.native_pay(NativeParams::new(
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
```rust
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
JsapiResponse {
    code: None,
    message: None,
    prepay_id: Some("wx201410272009395522657a690389285100")
}
```
## app支付

```rust
use wechat_pay_rust_sdk::model::AppParams;
use wechat_pay_rust_sdk::pay::WechatPay;

let wechat_pay = WechatPay::from_env();
let body = wechat_pay.app_pay(AppParams::new(
    "测试支付1分",
    "1243243",
    1.into()
    )).expect("app_pay error");
println!("body: {:?}", body);
```
输出
```rust
AppResponse {
    code: None,
    message: None,
    prepay_id: Some("wx201410272009395522657a690389285100")
}
```
## 小程序支付

```rust
use wechat_pay_rust_sdk::model::MicroParams;
use wechat_pay_rust_sdk::pay::WechatPay;

let wechat_pay = WechatPay::from_env();
let body = wechat_pay.micro_pay(MicroParams::new(
    "测试支付1分",
    "1243243",
    1.into(),
    "open_id".into()
    )).expect("micro_pay error");
println!("body: {:?}", body);
```
输出
```rust
MicroResponse {
    code: None,
    message: None,
    prepay_id: Some("wx201410272009395522657a690389285100")
}
```

## 支付回调解密
```rust
use wechat_pay_rust_sdk::pay::{PayNotifyTrait, WechatPay};
let associated_data = "transaction";
let nonce = "gZiqzlfayUu2";
let ciphertext = "pCidqdiS5IIj5f9Pw9j69zuzu8l8IxcPCkfsTBKzna4gqZztNAqTMUY/Ai0rtj8qhaX0naYZF3a2lRid/ofK/83MNv+Neb5+w/0+UOO9nLNJvIFy3oFeMf2PTbp6tgDE35T5AoP9iKQ+1VkXTiUdRxzFoRx6/LfBzHmeuVEDHKScRqjrf6NdxuDDD0ciCQaiHmb18Y0BRZdfNxWTAC83Rar5yTX2NNZPBtGdFDG3yAK2I3Vp7ZKLeMa92ecExNGwHrdJ+HxWw66IIdwVqJLlNmTG0c5zUpSc8yovnaJi1Wv/TC7Tm5NzcwdHsdRE110tIWFbvNmIzIIb+3P33JFWmaXXb1VVDC43DqtlplttYwL6H3kU0ABgHMMbccTwYmP4cSY8BCAL01754nqipxWogEC/la9iQiw85+rLRo/Ny9k3mp8n35D6bDNtS1LiaslbLM92ZbfKeglTg54F/R1l5xWolAVpx8iTz8Oc+XJClXdWr8j5poyh8zK2/RrXPRfr+8s2/oGeGvdaqJbN/LviYcCMDbXU9pKDScWlSi4akxfJu0EatPDvFEbn5DYRQnn5v6wCeesYkEL+wiFCAIs=";
let wechat_pay = WechatPay::from_env();
let data = wechat_pay.decrypt_paydata(
    ciphertext,
    nonce,
    associated_data
).unwrap();
println!("data: {:#?}", data);
```
解密结果
```
WechatPayDecodeData {
    mchid: "163971811111",
    appid: "wx15f4803f25xxxxx",
    out_trade_no: "8e289eebd1f44604b0b27e05f11bcf10",
    transaction_id: "4200001926202401125681342683",
    trade_type: "MWEB",
    trade_state: "SUCCESS",
    trade_state_desc: "支付成功",
    bank_type: "OTHERS",
    attach: "",
    success_time: "2024-01-12T10:36:13+08:00",
    payer: PayerInfo {
        openid: "oAZUY6DittOj59wCzPn6vNgpK2eY",
    },
    amount: AmountInfo {
        total: 1,
    },
}
```
**actix-web demo**
```rust
// 支付回调json格式为：{"id":"376151be-0eac-5047-b08a-46b52e15d2e2","create_time":"2024-01-12T12:17:33+08:00","resource_type":"encrypt-resource","event_type":"TRANSACTION.SUCCESS","summary":"支付成功","resource":{"original_type":"transaction","algorithm":"AEAD_AES_256_GCM","ciphertext":"u+MVmYPLQO4fjRsGWChm3sc/AXFVsytCI362RzYJyG25RbP6RSxYtkC2TIUA2ECfdhaJ0pIYuv4TwHwB1JE+0dn/MVQIjsBgaL9jx6IxmFIbkvNg0o623PF250ZhC9snTzxKJJtPtKFn3E8bR/pmqO4zbwUjQyQI5B4LqmzFcKpiKqGZSyG0BdvEWV2sDlR8oHD3s5RH/YN6c0aI7pEtVa1n7CR4qqQo9/NLAjTwloXWxB0BB+OnmlXQ9fu1UdJBS8L53W9zpREbEpH3BeCjrML/5qBs2nwcgvRV0OM30LkEdX8/lX7PiR6jzT2SexbinpSzx1QyXy9ZZfLRjFWVfQDTcDOrkMIaem4rhRgkAe5UDx6xdtqbgPSi5Ry/KHPm1+ptAl1GmEe9LIz8fRLleew3U0THXTSjnu5dJaXqk0qEizvK1pQBZ97QuzWuC2sVh4pd/OyqSNn93mlslkJIgT/UjQRcTIUE/CphdI7BGJkKYbEz4pSoqD/lxUiZNlMWbDeP4gEu/B7+Uk8n9vCOzR35VroLpweC0aDnCa3ru8DfMOcLQTvq04M4GJha9aodXec399ma3UcLEuw=","associated_data":"transaction","nonce":"pEw6yyO8XiSj"}}
// 自行使用web框架获取post json数据，下面演示actix-web获取并解析
// `WechatPayNotify` sdk中自带
#[post("/pay/notify")]
async fn pay_notify(data: Json<WechatPayNotify>, req: HttpRequest) -> impl Responder {
    let data = data.into_inner();
    let nonce = data.resource.nonce;
    let ciphertext = data.resource.ciphertext;
    let associated_data = data.resource.associated_data.unwrap_or_default();
    dotenv().ok();
    let wechat_pay = WechatPay::from_env();
    let result: WechatPayDecodeData = wechat_pay.decrypt_paydata(
        ciphertext, //加密数据
        nonce, //随机串
        associated_data, //关联数据
    ).unwrap();
    debug!("result: {:#?}", result);
    HttpResponse::Ok().json(serde_json::json!({
        "code": "SUCCESS",
        "message": "成功"
    }))
}
```
!*/

cfg_if::cfg_if! {
    if #[cfg(feature = "blocking")] {
        pub mod blocking;
    } else if #[cfg(feature = "async")] {
        pub mod async_impl;
    }
}
pub mod pay;
pub mod pay_type;
pub mod sign;
pub mod util;
pub mod request;
pub mod error;
pub mod model;
pub mod response;