# wechat-pay-rust-sdk
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
7. [读取平台证书](#读取平台证书)
8. [签名验证](#签名验证)

# 使用指南
引入依赖
```toml
#异步
wechat-pay-rust-sdk = {version = "x.x.x"}
# 同步
wechat-pay-rust-sdk = {version = "x.x.x", features = ["blocking"]}
# debug日志开启
wechat-pay-rust-sdk = {version = "x.x.x", features = ["blocking","debug-print"]}
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
```
H5Response { 
    code: None, 
    message: None, 
    h5_url: Some("https://wx.tenpay.com/cgi-bin/mmpayweb-bin/checkmweb?prepay_id=wx11154002858116623fasdfasdf&package=760499411") 
}
```
h5_url转换成微信支付链接
```rust
let body = wechat_pay.h5_pay(H5Params::new(
    "测试支付1分",
    util::random_trade_no().as_str(),
    1.into(),
    H5SceneInfo::new("183.6.105.141", "软件", "https://mydomain.com"),
)).expect("h5_pay error");
let weixin_url = wechat_pay.get_weixin(body.h5_url.unwrap().as_str(), "https://mydomain.com").unwrap();
println!("weixin_url: {}", weixin_url.unwrap());
```
输出
```
weixin://wap/pay?prepayid%3Dwx13013716281xa5df8313490000&package=35748946&noncestr=1705081036&sign=8d988c82ded5fb02f097d6f1d70
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
let body = wechat_pay.micro_pay(JsapiParams::new(
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
## actix-web demo
支付回调json格式为
```json
{"id":"376151be-0eac-5047-b08a-46b52e15d2e2","create_time":"2024-01-12T12:17:33+08:00","resource_type":"encrypt-resource","event_type":"TRANSACTION.SUCCESS","summary":"支付成功","resource":{"original_type":"transaction","algorithm":"AEAD_AES_256_GCM","ciphertext":"u+MVmYPLQO4fjRsGWChm3sc/AXFVsytCI362RzYJyG25RbP6RSxYtkC2TIUA2ECfdhaJ0pIYuv4TwHwB1JE+0dn/MVQIjsBgaL9jx6IxmFIbkvNg0o623PF250ZhC9snTzxKJJtPtKFn3E8bR/pmqO4zbwUjQyQI5B4LqmzFcKpiKqGZSyG0BdvEWV2sDlR8oHD3s5RH/YN6c0aI7pEtVa1n7CR4qqQo9/NLAjTwloXWxB0BB+OnmlXQ9fu1UdJBS8L53W9zpREbEpH3BeCjrML/5qBs2nwcgvRV0OM30LkEdX8/lX7PiR6jzT2SexbinpSzx1QyXy9ZZfLRjFWVfQDTcDOrkMIaem4rhRgkAe5UDx6xdtqbgPSi5Ry/KHPm1+ptAl1GmEe9LIz8fRLleew3U0THXTSjnu5dJaXqk0qEizvK1pQBZ97QuzWuC2sVh4pd/OyqSNn93mlslkJIgT/UjQRcTIUE/CphdI7BGJkKYbEz4pSoqD/lxUiZNlMWbDeP4gEu/B7+Uk8n9vCOzR35VroLpweC0aDnCa3ru8DfMOcLQTvq04M4GJha9aodXec399ma3UcLEuw=","associated_data":"transaction","nonce":"pEw6yyO8XiSj"}}
```
自行使用web框架获取post json数据
```rust
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

## 读取平台证书
```rust
use wechat_pay_rust_sdk::pay::WechatPay;

let wechat_pay = WechatPay::from_env();
let response = wechat_pay.certificates().expect("certificates error");
println!("response: {:#?}", response);
```
响应
```json
CertificateResponse {
    data: Some(
        [
            Certificate {
                serial_no: "32507F67D05E9443E39ED3E7D5DBF21BB44E5D0C",
                effective_time: "2023-03-22T22:58:57+08:00",
                expire_time: "2028-03-20T22:58:57+08:00",
                encrypt_certificate: EncryptCertificate {
                    algorithm: "AEAD_AES_256_GCM",
                    nonce: "034246e50ad4",
                    associated_data: "certificate",
                    ciphertext: "/jXFSfxLXhdWii/ArpvW/XTRECZ5I1RoYcrw/At5w0oey+xl35BpWxPP+YoD+8GutY7NUJ2RgNfoAIrNdZhATO4ZxHASh93U06rKJOAnBVJMS18YCvk0TDdyoVkk9RFhKfEq9fdsUjxJ29gosHVcOHGgATR8OZmbpasrWnoAPLU9sEcZanRm5d5Ig3QLPkoBr1GKnX7bDLN8loUitq3+tCzSrDYQB8+SUWEqxjAHxoSy6zb46Vh7T1dJntGFdcCL499e2+8imm5y7XG9B4d69J0U9/a7OhnFMpcVZanFPv+4Y6jZf44isBzIwF5yz32DZ+zXLW46Yuef48DUJOJnHVmoP/R5AY1oBAuwwbZY+hlQNkqmZbbq5NTkxyxk2Qxn9NaVeZv8dKHDMt29MEX+Uzz6uA4k0Gdqd//gIe6dGgHsMrte7fSwNvS0v2Re/jPBNh9AfYpPnlo/J4Cw9T2Eb3BPAnH197mw0Gvc+tMJVDR1II8vBGjXjExhdEVNlQTPeEJh1qvTmfNLr9QfYi8AW2lo3TjOVtEhrqUumMUT4F+RxL0/AiE+sIdTvj0DH7pTvVe9nJ9pR0cbnleBUXvYvYZxl/moBekp/GVbq1XRpDwX/SApzyqW/6ZOPn9gOK63xCm09qlihoXtC1cjKnv6s7ozmqTsyqyD6gk5QkY303HlrQ0EvqfFm9HnYA3ycr4Eh0808QdaELnIB9MCMjb00EvWobvRy02SO0Y5bAV1Ea9SRWcowv3mqtjMx6gBgLxt1U0+qWENFm9CAK2I6gA0tDFGgOvBH12rwTwKePa0efYuleK+y2RV5/7CQ+wFSPjYXITaI37tO8F6iD8mUx0ARCKAoy5ruliuZHBQvllKdg0rkvRABU/UldGildkf5RZ2dyy0MjPmI84Sm5HPpI9qr4VJb/8ZLOebTPHRKIgUks75eAYkJiB6KfuER8Js/4f3hKBcPxdX49lhHm16sp3Layxthv/YAfEeNVTX47qODarl1qtjIstnY8SMHin7XuXflE2JZKDWny63ssnqzgMN0KbLxBtyUzbd3HmAEK2t4fRQgrVyePM5KJ+Zk+s+pGG8UTG8EoysznO/EzZybyttj3MPWhkAVhGMeo9B8uCuuwx1PsSAeYXzvyYRAUqc5Fqh3PfIPYnNFtwd2iEdV1yKFhiNozwp1soOFp1hm7+2o9M7FsDxtofOJ6DMkTbb9/Ba8QIqEb3tiVw0uG340wbPTv8DNvJJUTzL8JdOlE0q4Dlq6rrV+09WamOGzmTMDsoKjFraFc+8XO3PVu0oQ179Zaf37IZrwQshvLH0hHot30dHBz235iW+QW5hMEeMM+Uz/EmWEBj0V4VorJBGfIWA+iy/bDWMfmcZeHGA91ITLDYnxifWu6XGIEKtAt2Y3lsIvfcjtcxLbwgHUMl9v7rewHsGYxFpcG35Y5Yk/uxrj7acCHWVgpBaH2ShCklLrUixqeTfsgBJ7OSysZM7UqWThtzU1GBUqplrHQCLzL2t6YAd8XJWc0mUa6VsfqWuQQZ5HDcnsYExKHExq7LbKf+z+5J2FjYqFWU16cPkndQbBjbOGQqaFd+1BbVU1ZkkUIMxYTKgIo3w4Z3hnlEHSBeb8psD9k9kB4ol831Vp0fH3aGR+a3/uLZaI1sUS5gM6OzsmF8X0nU8XfsVmLq0QK9AvNI97qCwS9zC/3U+KazaYpTCYpwhq9u5nB1Qtbwc95+tlpA+A/2hzPd+ym+Yv5z0JevYbVicO9S0awqAkxIXChQa56QPiLQ0cezPJN2bc+/we2wsoPxVHMWhLj9q2rQojKUycBNMcnK0X0081KaVlTxZ1GB+a0UgSuO+l05tIr0fO03+Eme6huUKSZVC9MOLbGtGBpUFyv0CclDCgiLwjDGFi8+vNyRoQD32Y0zAdRPrKA2K+cINKxK739jBfc/ZjMsYfo8W8HeTTq5tI+DyY734Yo3XgCH/EcZNmsqe1JetkeK+",
                },
            },
        ],
    ),
}
```
解密上面的证书
```rust
use wechat_pay_rust_sdk::pay::{PayNotifyTrait, WechatPay};
use wechat_pay_rust_sdk::response::Certificate;

let wechat_pay = WechatPay::from_env();
let response = wechat_pay.certificates().expect("certificates error");
let data: Certificate = response.data.unwrap()[0].clone();
let ciphertext = data.encrypt_certificate.ciphertext;
let nonce = data.encrypt_certificate.nonce;
let associated_data = data.encrypt_certificate.associated_data;
let data = wechat_pay.decrypt_bytes(ciphertext, nonce, associated_data).unwrap();
println!("cer: {}", String::from_utf8(data).unwrap());
```
输出证书
```text
-----BEGIN CERTIFICATE-----
MIIEFDCCAvygAwIBAgIUMlB/Z9BelEPjntPn1dvyG7ROXQwwDQYJKoZIhvcNAQEL
BQAwXjELMAkGA1UEBhMCQ04xEzARBgNVBAoTClRlbnBheS5jb20xHTAbBgNVBAsT
FFRlbnBheS5jb20gQ0EgQ2VudGVyMRswGQYDVQQDExJUZW5wYXkuY29tIFJvb3Qg
Q0EwHhcNMjMwMzIyMTQ1ODU3WhcNMjgwMzIwMTQ1ODU3WjBuMRgwFgYDVQQDDA9U
ZW5wYXkuY29tIHNpZ24xEzARBgNVBAoMClRlbnBheS5jb20xHTAbBgNVBAsMFFRl
xxxxxxx.....
-----END CERTIFICATE-----
```
将上面的cert.pem证书转pem公钥
```shell
openssl x509 -pubkey -noout -in cert.pem > pubkey.pem
```
输出公钥
```text
-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA4zej1cqugGQtVSY2Ah8RMCKcr2UpZ8Npo+5Ja9xpFPYkWHaF1Gjrn3d5kcwAFuHHcfdc3yxDYx6+9grvJnCA2zQzWjzVRa3BJ5LTMj6yqvhEmtvjO9D1xbFTA2m3kyjxlaIar/RYHZSslT4VmjIatW9KJCDKkwpM6x/RIWL8wwfFwgz2q3Zcrff1y72nB8p8P12ndH7GSLoY6d2Tv0OB2+We2Kyy2+QzfGXOmLp7UK/pFQjJjzhSf9jxaWJXYKIBxpGlddbRZj9PqvFPTiep8rvfKGNZF9Q6QaMYTpTp/uKQ3YvpDlyeQlYe4rRFauH3mOE6j56QlYQWivknDX9VrwIDAQAB
-----END PUBLIC KEY-----
```
## 签名验证
使用上面的公钥用来验签
```rust
use wechat_pay_rust_sdk::pay::{PayNotifyTrait, WechatPay};

let wechat_pay = WechatPay::from_env();
let pub_key = std::fs::read_to_string("pubkey.pem").unwrap();
//在支付回调中header获取到的签名
let wechatpay_signatrue = "mFgmwXAKL3YJj34b7f+cUG3vkW09TiXU4lOSzCbvWFtvyLTb5WiyfAiVXZmMB17Qh9gDVkqboO97zfIYfv+AVdxj3GQljWlW+vE1Ujn2uxiFld6bWwz8Znk+833ruzZ8mAIaqLEjI/HKuVPdTj4LFzh/EO+gEMR6WDXr+7cZV7D3qUTXuO26fHLe0PmleDziG8SPgYjihK1ztF3Os0NhvL5tQMM8LKDOMzO3kxSr/TqTBtsB/OnuP2mH8yaSUeYeTpGStYvSw8KVi+gk6VnrlkVmdFh3DDXY60GCzCZ8zPl12RmzZbBRSK8ocVrzs4tuqRa5Euk3cDIA6qHqS8hyBQ==";
//回调的数据
let body = r#"{"id":"29a61973-babf-599a-966d-6bcdcf17360c","create_time":"2024-01-12T21:39:44+08:00","resource_type":"encrypt-resource","event_type":"TRANSACTION.SUCCESS","summary":"支付成功","resource":{"original_type":"transaction","algorithm":"AEAD_AES_256_GCM","ciphertext":"5ZfDK+LRJakAkC7kdHKRzCu5WZ0JFC2qSwP4InWNFeUnY0uaOnzfCjiqhDTFYyP4ywxuLxPUOiVI3WT6CcU0NNqbadTQ5XzjVuKLxYSnOYCFULltIrfsT/mUF4VW+xBMgSgG4+ZdzhRXVr+AzihDKFjw2p1iCtLYz9emgToctygNBtV6JDEI2BnCoiEM7qyIU1ALv5IsufQHDQqzjYXd16OD3i6O8UeSE2GOd4ifmQrAKGKalwWPECI73/qTFoAcLcgbhhn1TeSEaHoF7xceDmkL9AGlC21pBwYWoibTgqdlDJiz3IctrCzH6PPXD8XcApEj4A3ByyPjaNs6HxaJGzEHYGUkyM2/b7SzZIzqlBmNRZYFvBC0BOwoktyxrIhg3bKSbYtDYt1+8lMaYIJW6Dgq9GjG6pxAVrYULt8sk8cKZ+OrK9iXHZI11pYyK9YwWJLXbs6GyjMdDxhaGilF9csK8ZSsKzUjvlcLCjboCFX6nuHvCbswchYchQhTeitKDKG3/q+4snY183dBA6rXBHKQduqc1vXRR6odMcU1Evvy5mKnDTDELlI6mqvBtJ10XNED5O43ga5ZAODxYoU=","associated_data":"transaction","nonce":"uaGeNnBYNjl7"}}"#;
//支付回调中header获取到的时间戳
let wechatpay_timestamp = "1705066785";
//支付回调中header获取到的随机串
let wechatpay_nonce = "Jh9oPZelCJIQeQ47kz4stzvDKpLEUhCX";
wechat_pay.verify_signatrue(
    pub_key.as_str(),
    wechatpay_timestamp,
    wechatpay_nonce,
    wechatpay_signatrue,
    body,
).unwrap();
```
actix-web中验证的例子
```rust
#[post("/pay/notify")]
async fn pay_notify(bytes: Bytes, req: HttpRequest) -> impl Responder {
    let headers = req.headers();
    let pub_key = std::fs::read_to_string("pubkey.pem").unwrap();
    let wechatpay_signatrue = headers.get("wechatpay-signature").unwrap().to_str().unwrap();
    let wechatpay_timestamp = headers.get("wechatpay-timestamp").unwrap().to_str().unwrap();
    let wechatpay_nonce = headers.get("wechatpay-nonce").unwrap().to_str().unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let wechat_pay = WechatPay::from_env();
    wechat_pay.verify_signatrue(
        pub_key.as_str(),
        wechatpay_timestamp,
        wechatpay_nonce,
        wechatpay_signatrue,
        body,
    ).expect("签名验证失败，非法数据");
    HttpResponse::Ok().json(serde_json::json!({
        "code": "SUCCESS",
        "message": "成功"
    }))
}
```
