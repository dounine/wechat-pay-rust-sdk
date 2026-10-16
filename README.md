# wechat-pay-rust-sdk

微信支付 APIv3 的 Rust SDK（**社区维护，非腾讯官方**）。

覆盖：JSAPI / Native / APP / H5 / 付款码下单、申请退款、订单查询、关单、退款查询、交易账单（对账）、
平台证书获取与轮换、**微信支付公钥与平台证书双表并存（灰度期两种签名都认）**、
支付回调的验签与解密、**出站应答验签（默认强制）**。

[![QQ群](https://img.shields.io/badge/QQ%E7%BE%A4-799168925-blue)](http://qm.qq.com/cgi-bin/qm/qr?_wv=1027&k=dLoye8pBcO60zGzqLjGO0l-GgMIaf6wQ&authKey=LfxBdZ5A%2F9eWJbKpzTcuWPjmQu5UdIJ3TVTpqRAQYkCID50WLkYoIXcGxGKzupG3&noverify=0&group_code=799168925)

# API文档
- [wechat-pay-rust-sdk](#wechat-pay-rust-sdk)
- [API文档](#api文档)
- [使用指南](#使用指南)
  - [native支付](#native支付)
  - [h5支付](#h5支付)
  - [jsapi支付](#jsapi支付)
  - [app支付](#app支付)
  - [小程序支付](#小程序支付)
  - [支付回调解密](#支付回调解密)
  - [actix-web demo](#actix-web-demo)
  - [读取平台证书](#读取平台证书)
  - [签名验证](#签名验证)
  - [退款申请](#退款申请)
  - [订单查询 / 关单 / 退款查询](#订单查询--关单--退款查询)
  - [回调通知：验签与防重放](#回调通知验签与防重放)
  - [应答验签（默认强制）](#应答验签默认强制)
  - [平台证书轮换](#平台证书轮换)
  - [错误处理](#错误处理)
  - [超时与连接复用](#超时与连接复用)
  - [自动重试](#自动重试)
  - [主备域名（跨城冗灾）](#主备域名跨城冗灾)

# 使用指南
引入依赖

```toml
# 同步（默认）：blocking 客户端
wechat-pay-rust-sdk = "0.3.0"

# 异步：与同步版同名的方法返回 Future；下游需自带带 timer 的 tokio runtime
wechat-pay-rust-sdk = { version = "0.3.0", features = ["async"] }

# 调试日志：会打印请求体与 Authorization 头（含 openid、金额），生产环境不要开
wechat-pay-rust-sdk = { version = "0.3.0", features = ["debug-print"] }

# 边改 SDK 边调业务代码：path = "../wechat-pay-rust-sdk"（features 同上）
```

> ⚠ `0.2.x` → `0.3.0` 含**破坏性改动**（`WechatPay::new(...)` → `WechatPayConfig` + `from_config`、
> 删除 `WeChatResponse<T>`、默认强制应答验签、退款重试走独立的分钟级策略等），
> 升级前请先读 [CHANGELOG](CHANGELOG.md)。
>
> MSRV 1.89（edition 2024）。
> ⚠ 不存在 `blocking` feature —— 同步是**默认**行为，异步才需要 feature。

## native支付
```rust
use wechat_pay_rust_sdk::model::NativeParams;
use wechat_pay_rust_sdk::pay::{WechatPay, WechatPayConfig};

let private_key_path = "./apiclient_key.pem";
let private_key = std::fs::read_to_string(private_key_path).unwrap();
let wechat_pay = WechatPay::from_config(WechatPayConfig {
    appid: "app_id".into(),
    mch_id: "mch_id".into(),
    private_key,
    serial_no: "serial_no".into(),
    v3_key: "v3_key".into(),
    notify_url: "notify_url".into(),
});
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
println!("weixin_url: {}", weixin_url);
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
    prepay_id: Some("wx201410272009395522657a690389285100") 
}
 ```

## 小程序支付

小程序支付走的就是 **JSAPI 下单**接口，所以用 `jsapi_pay` —— 与公众号 JSAPI 的唯一区别是
`openid` 来自小程序的 `wx.login`（前端拿 `code`，后端调
`https://api.weixin.qq.com/sns/jscode2session` 换 `openid`；那是小程序 API，不在本 SDK 范围内）。

```rust
use wechat_pay_rust_sdk::model::JsapiParams;
use wechat_pay_rust_sdk::pay::WechatPay;

let wechat_pay = WechatPay::from_env();
let response = wechat_pay.jsapi_pay(JsapiParams::new(
     "测试支付1分",
     "1243243",
     1.into(),
     "oXXXX-xxxxxxxx".into(),   // 小程序的 openid（由 jscode2session 换来）
     )).expect("jsapi_pay error");

let sign_data = response.sign_data.expect("下单成功必有签名数据");

// 序列化出来就是官方键名（timeStamp / nonceStr / package / signType / paySign / appId），
// 小程序只差一件事：官方参数表里没有 appId，删掉再交给前端。
let mut args = serde_json::to_value(&sign_data).expect("SignData 必然可序列化");
args.as_object_mut()
    .expect("SignData 序列化后是 JSON 对象")
    .remove("appId");
println!("{args}");
```

输出（交给前端 `wx.requestPayment(args)` 即可拉起支付）

```json
{"nonceStr":"5K8264ILTKCH16CQ2502SI8ZNMTM67VS","package":"prepay_id=wx201410272009395522657a690389285100","paySign":"oR9d8PuhnIc+YZ8cBHFCwfgpaK9gd7vaRvkYD7rthRAZ/X+QBilZosN16P9toCpAcqeJ977dGOz01C80C/Z9C1w==","signType":"RSA","timeStamp":"1414561699"}
```

> ⚠ 字段名的大小写由官方定死（`timeStamp` 的 `S` 是大写），**别手写** —— 拼错时前端只会报
> 「缺少参数」，不会指向你。
> ⚠ 小程序**不要**传 `appId`（官方的小程序参数表里没有它，那是公众号 JSAPI 才需要的）。
> ⚠ **APP 支付不能复用这套键名**：APP SDK 用的是 `appid` / `partnerid` / `prepayid` /
> `package` / `noncestr` / `timestamp` / `sign`。

## 支付回调解密

> ⚠ **解密不能替代验签**，下面只是演示解密出来的数据长什么样。
> 真实的回调入口必须**先验签再解密**：解密（AEAD）虽然能挡住伪造的密文，但挡不住**重放** ——
> 抓到一次合法回调就能反复投递。而且这段用了 `.unwrap()`，出错会变成 500 让微信一直重投。
> 正确顺序与完整示例见「[回调通知：验签与防重放](#回调通知验签与防重放)」，
> 可直接运行的版本见 `example/src/main.rs`。
> 推荐入口是 `wechat_pay.decrypt_notify(&notify.resource)` —— 它会先校验 `resource.algorithm`。

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
    attach: Some(""),
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

> ⚠ 下面这个 handler **只解密、不验签**，不要直接拿去用于生产：它没有防重放，
> 且每个请求都 `WechatPay::from_env()` 重建一次客户端（连连接池一起重建）。
> 可直接运行的完整参考实现（**验签 → 解密 → 幂等 → 应答**）见 `example/src/main.rs`。

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
    tracing::debug!("result: {:#?}", result);
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
let pub_key = util::x509_to_pem(data.as_slice()).unwrap(); //证书转公钥
let mut pub_key_file = std::fs::File::create("pubkey.pem").unwrap();
pub_key_file.write_all(pub_key.as_bytes()).unwrap();

let (pub_key_valid, expire_timestamp) = util::x509_is_valid(data.as_slice()).unwrap();
tracing::debug!("pub key valid:{} expire_timestamp:{}", pub_key_valid, expire_timestamp);//检测证书是否可用,打印过期时间
println!("pub key: {}", pub_key);
```
输出公钥
```text
-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA5ufQLGNv3VlQObbWxoRU
c+8XLaogBL58i8cjapp5YqqS33cN7Qtcmiob5baV84Iu6DMClgWKB9VSaMs7vX6w
7WM++CPxOyg3oewLKDNPBvPtXBATWRScrmFh37XxXt2wJdLItdl86jd8o0sX8o7K
w4TIXKxlR7YQ3sxNWlbCpd4g3gkcXiV0erIbzwEn5d5n74xOTjnmbXf1FTN3K3HA
xxxx...
-----END PUBLIC KEY-----

```
## 签名验证
使用上面的公钥用来验签
> 平台的证书有时效性，请及时检测并下载最新的证书并替换本地公钥。
>
> ⚠ 本节讲的是**手工验签**（回调场景）。**出站请求的应答验签由 SDK 自动完成**，
> 默认强制，见「应答验签」一节 —— 那里不需要你手工传公钥。

```rust
use wechat_pay_rust_sdk::pay::{PayNotifyTrait, WechatPay};

let wechat_pay = WechatPay::from_env();
let pub_key = std::fs::read_to_string("pubkey.pem").unwrap();
//在支付回调中header获取到的签名
let wechatpay_signature = "mFgmwXAKL3YJj34b7f+cUG3vkW09TiXU4lOSzCbvWFtvyLTb5WiyfAiVXZmMB17Qh9gDVkqboO97zfIYfv+AVdxj3GQljWlW+vE1Ujn2uxiFld6bWwz8Znk+833ruzZ8mAIaqLEjI/HKuVPdTj4LFzh/EO+gEMR6WDXr+7cZV7D3qUTXuO26fHLe0PmleDziG8SPgYjihK1ztF3Os0NhvL5tQMM8LKDOMzO3kxSr/TqTBtsB/OnuP2mH8yaSUeYeTpGStYvSw8KVi+gk6VnrlkVmdFh3DDXY60GCzCZ8zPl12RmzZbBRSK8ocVrzs4tuqRa5Euk3cDIA6qHqS8hyBQ==";
//回调的数据
let body = r#"{"id":"29a61973-babf-599a-966d-6bcdcf17360c","create_time":"2024-01-12T21:39:44+08:00","resource_type":"encrypt-resource","event_type":"TRANSACTION.SUCCESS","summary":"支付成功","resource":{"original_type":"transaction","algorithm":"AEAD_AES_256_GCM","ciphertext":"5ZfDK+LRJakAkC7kdHKRzCu5WZ0JFC2qSwP4InWNFeUnY0uaOnzfCjiqhDTFYyP4ywxuLxPUOiVI3WT6CcU0NNqbadTQ5XzjVuKLxYSnOYCFULltIrfsT/mUF4VW+xBMgSgG4+ZdzhRXVr+AzihDKFjw2p1iCtLYz9emgToctygNBtV6JDEI2BnCoiEM7qyIU1ALv5IsufQHDQqzjYXd16OD3i6O8UeSE2GOd4ifmQrAKGKalwWPECI73/qTFoAcLcgbhhn1TeSEaHoF7xceDmkL9AGlC21pBwYWoibTgqdlDJiz3IctrCzH6PPXD8XcApEj4A3ByyPjaNs6HxaJGzEHYGUkyM2/b7SzZIzqlBmNRZYFvBC0BOwoktyxrIhg3bKSbYtDYt1+8lMaYIJW6Dgq9GjG6pxAVrYULt8sk8cKZ+OrK9iXHZI11pYyK9YwWJLXbs6GyjMdDxhaGilF9csK8ZSsKzUjvlcLCjboCFX6nuHvCbswchYchQhTeitKDKG3/q+4snY183dBA6rXBHKQduqc1vXRR6odMcU1Evvy5mKnDTDELlI6mqvBtJ10XNED5O43ga5ZAODxYoU=","associated_data":"transaction","nonce":"uaGeNnBYNjl7"}}"#;
//支付回调中header获取到的时间戳
let wechatpay_timestamp = "1705066785";
//支付回调中header获取到的随机串
let wechatpay_nonce = "Jh9oPZelCJIQeQ47kz4stzvDKpLEUhCX";
wechat_pay.verify_signature(
    pub_key.as_str(),
    wechatpay_timestamp,
    wechatpay_nonce,
    wechatpay_signature,
    body,
).unwrap();
```
actix-web中验证的例子
```rust
#[post("/pay/notify")]
async fn pay_notify(bytes: Bytes, req: HttpRequest) -> impl Responder {
    let headers = req.headers();
    let pub_key = std::fs::read_to_string("pubkey.pem").unwrap();
    let wechatpay_signature = headers.get("wechatpay-signature").unwrap().to_str().unwrap();
    let wechatpay_timestamp = headers.get("wechatpay-timestamp").unwrap().to_str().unwrap();
    let wechatpay_nonce = headers.get("wechatpay-nonce").unwrap().to_str().unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let wechat_pay = WechatPay::from_env();
    wechat_pay.verify_signature(
        pub_key.as_str(),
        wechatpay_timestamp,
        wechatpay_nonce,
        wechatpay_signature,
        body,
    ).expect("签名验证失败，非法数据");
    HttpResponse::Ok().json(serde_json::json!({
        "code": "SUCCESS",
        "message": "成功"
    }))
}
```

## 退款申请

```rust
    use wechat_pay_rust_sdk::error::PayError;
    use wechat_pay_rust_sdk::model::RefundsParams;
    use wechat_pay_rust_sdk::pay::WechatPay;
    
    let wechat_pay = WechatPay::from_env();
    let req = RefundsParams::new("123456", 1, 1, None, Some("123456"));
    match wechat_pay.refunds(req).await {
        // 受理成功 ≠ 退款成功，需再用 query_refund 轮询 status 到终态
        Ok(body) => tracing::debug!("refunds status: {} refund_id: {}", body.status, body.refund_id),
        // 微信的业务错误：非 2xx 与「200 但 body 是错误信封」都会走到这里
        // request_id 来自应答头 Request-ID —— 向微信侧排查问题时提供该值
        Err(PayError::ApiError { status, response, request_id }) => tracing::debug!(
            "refunds failed: http {status}, request-id={}, code={:?}, message={:?}, detail={:?}",
            request_id.as_deref().unwrap_or("-"),
            response.code, response.message, response.detail
        ),
        Err(e) => tracing::debug!("refunds error: {e}"),
    }

```

## 订单查询 / 关单 / 退款查询

```rust
// 查单：GET /v3/pay/transactions/out-trade-no/{no}?mchid=…
let order = wechat_pay.query_order("ORDER_0001").await?;
// trade_state: SUCCESS / REFUND / NOTPAY / CLOSED / REVOKED / USERPAYING / PAYERROR
if order.trade_state == "SUCCESS" {
    // transaction_id / amount / payer 只在支付成功后才有值
    println!("已支付: {:?}", order.transaction_id);
}

// 关单：已支付的订单不能关，只能退。成功时微信返回 204，本方法返回 Ok(())
wechat_pay.close_order("ORDER_0001").await?;

// 退款查询：退款是异步的，申请受理后要轮询到终态
let refund = wechat_pay.query_refund("REFUND_0001").await?;
match refund.status.as_str() {
    "SUCCESS" => { /* 退款成功 */ }
    "PROCESSING" => { /* 官方建议每分钟查一次，5 分钟后降频 */ }
    "ABNORMAL" | "CLOSED" => { /* 需要人工介入 */ }
    _ => {}
}
```

⚠ 查单的 `mchid` 放在 **query string** 里、且**参与签名**；关单的 body **只有** `mchid`。
这两点决定了它们不能复用下单接口的字段注入逻辑（那是 `pay()` 独有的）。

订单不存在时微信返回 404 `ORDER_NOT_EXIST`，会变成 `Err(PayError::ApiError)` ——
这是**正常业务结果**，不是故障。

## 回调通知：验签与防重放

```rust
use wechat_pay_rust_sdk::model::WechatPayNotify;
use wechat_pay_rust_sdk::notify::NotifyHeaders;
use wechat_pay_rust_sdk::pay::PayNotifyTrait; // decrypt_notify 是 trait 方法

// 启动时拉一次平台证书，之后每 12 小时内刷新
let mut keys = wechat_pay.fetch_platform_keys().await?;
let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;
keys.mark_refreshed(now);

// 回调入口；raw_body 必须是**原始请求体字符串**
let headers = NotifyHeaders::from_pairs(req_headers)?; // 任意框架的 (name, value) 迭代器
keys.verify_notify(&headers, raw_body)?;               // 新鲜度 → 按 serial 选键 → 验签

// 解密：入口会先校验 resource.algorithm（非 AEAD_AES_256_GCM 在解密前就报错）
let notify: WechatPayNotify = serde_json::from_str(raw_body)?;
let data = wechat_pay.decrypt_notify(&notify.resource)?;
// 然后按 out_trade_no 落库去重 —— 幂等必须你自己做
```

几点必须记住：

1. **`raw_body` 必须是原始字节。** 若框架先把 body 反序列化成 JSON、再序列化回去，
   字节变了，验签必然失败。用 `Bytes` 之类的类型拿原始体。
2. **验签不能省。** 微信会故意发 `WECHATPAY/SIGNTEST/` 开头的坏签名来探测你的验签实现 ——
   这是正常流量，直接拒绝即可，不要为它开特例。
3. **SDK 不替你做幂等。** 微信在收到成功应答前会重试（15s/15s/30s/3m/…
   最多 15 次），必须按 `out_trade_no` / `transaction_id` 落库去重后再发货。
4. **退款结果通知要换一个解码器。** `decrypt_notify` 解的是**支付**通知；退款通知
   （`REFUND.SUCCESS` / `REFUND.ABNORMAL` / `REFUND.CLOSED`）的字段不重合（没有
   `appid` / `trade_state`，多了 `out_refund_no` / `refund_status`），要用
   `decrypt_refund_notify(&notify.resource)` 解成 `WechatPayRefundDecodeData` —— 否则会以
   「缺字段」失败。两条通知的验签流程完全一样。
5. **`resource.algorithm` 不必自己比对。** 两个 `decrypt_*_notify` 入口会先校验它是
   `AEAD_AES_256_GCM`（微信回调的固定取值）：不是就在解密**之前**返回 `DecryptError`，
   而不是让你拿到一个方向错误的「GCM 解密失败」。
6. **宿主时钟是硬依赖。** ±300s 的新鲜度窗口与出站应答验签**共用** —— 主机时钟漂移会让
   所有回调与应答一起报 `StaleNotify`（看着像重放，其实是没对时，回调被拒就等于订单不发货）。
   部署基线要求 **NTP 同步**；`StaleNotify` 的日志要**单独告警**（它是运维信号，
   不要当成攻击结论）。

应答要求：**5 秒内**返回，成功时返回 HTTP **200 或 204 且不带 body**；
校验或处理失败才返回 4xx/5xx + `{"code":"FAIL","message":"…"}`。业务处理请异步化。

遇到 `PayError::UnknownPlatformSerial` 说明微信正在轮换证书 ——
**立即重新拉取**证书列表再重试，不要拿别的密钥去试。
（`Wechatpay-Serial` 以 `PUB_KEY_ID_` 开头的是**微信支付公钥**，它不在平台证书列表里：
拉取也拿不到，请用 `add_static_platform_key` 注入；这类 serial 不会触发任何证书请求。）

## 应答验签（默认强制）

微信不只给回调签名：**每个应答**都带 `Wechatpay-Serial` / `Wechatpay-Timestamp` /
`Wechatpay-Nonce` / `Wechatpay-Signature` 四个头（验签串是 `{时间戳}\n{随机串}\n{应答体}\n`，
与回调用的是同一套密钥：平台证书，加上调用方用 `add_static_platform_key` 注入的静态公钥 ——
两张表，静态优先）。本 SDK 默认**强制**校验它 —— 不验签的应答
等于把「这条应答真的来自微信」交给链路运气：一个能改应答的中间层可以伪造 `ORDER_NOT_EXIST`、
「请求未受理」这类结论，而调用方在超时兜底时正是靠它们决定要不要换个单号重开。

```rust
use wechat_pay_rust_sdk::pay::{ResponseVerify, WechatPayConfig};

let config = WechatPayConfig {
    appid: "wx123".into(),
    mch_id: "1900000001".into(),
    private_key: private_key_pem,
    serial_no: "5F2C…".into(),
    v3_key: "0123456789abcdef0123456789abcdef".into(),
    notify_url: "https://example.com/pay/notify".into(),
    response_verify: ResponseVerify::Required, // 默认行为，也是唯一推荐值
};
```

调用方不需要额外做什么：SDK 在首个请求前自动拉取平台证书（冷启动），之后按 12 小时窗口
刷新；遇到没见过的 `Wechatpay-Serial`（轮换期）会**刷新一次平台证书后重验** ——
重验用的是**已经收到的那份响应**，不会重发业务请求。

四条必须记住的：

1. **升级后若所有请求都报「缺少签名头」** —— 先查你的代理 / CDN 是否过滤了 `Wechatpay-*` 头
   （官方文档承认这是常见现象，建议改代理配置或直连），**不要**因此关掉验签。
2. **`ResponseVerify::Disabled` 只用于本地 mock 网关**，或网关暂时无法整改的场景；
   关掉之后应答不再被校验，也没有自动拉取密钥。
3. **验签失败不重试，但按「可能已生效」处置**：应答是**完整收到之后**才验签的，而且微信会
   故意在极少数应答里下发带 `WECHATPAY/SIGNTEST/` 前缀的错误签名来探测验签实现。
   拿到 `VerifyError` / `UnknownPlatformSerial` / `StaleNotify` 时
   `may_have_taken_effect()` 返回 `true` —— 写接口应当先去查单。
4. **微信支付公钥模式**（`Wechatpay-Serial` 形如 `PUB_KEY_ID_…`）必须自己配置公钥，
   它**不在**平台证书列表里，刷新也拿不到：

   ```rust
   let wechat_pay = wechat_pay.with_platform_public_key(
       "PUB_KEY_ID_0000000000000024101100397200006", // 商户平台给出的公钥 ID
       std::fs::read_to_string("wechat_pay_public_key.pem")?,
   );
   ```

   配置后客户端进入**静态密钥模式**：不自动拉取、不自动替换，公钥更新时再设一次。

   ⚠ **公钥 / 平台证书的灰度切换期请改用 `add_static_platform_key`**：官方切换期间同一商户的
   报文可能由**公钥**或**平台证书**随机签名，两种都得认。这个入口注入公钥但**不**切模式，
   因此平台证书的自动拉取 / 刷新照旧，并且官方网关下没有「每个请求前先拉一次证书」的强依赖
   （已注入静态密钥且轮换表为空时不预拉，改为遇到证书签名的应答时按需拉一次）：

   ```rust
   // 灰度期：公钥 + 平台证书双表并存（选键静态表优先）
   wechat_pay.add_static_platform_key(
       "PUB_KEY_ID_0000000000000024101100397200006",
       std::fs::read_to_string("wechat_pay_public_key.pem")?,
   )?;
   ```

   切换完成后想回到「只信这把公钥」，用 `set_platform_keys`（整体替换两张表并切静态模式）
   或重建客户端。

5. **排查两种「全部请求都失败」**：
   - 报「缺少签名头」→ 查代理 / CDN 是否过滤 `Wechatpay-*` 头（见上）；
   - 报 `StaleNotify`（时间戳偏差超 ±300s）→ 先**校时**（NTP）。应答验签用的是与回调
     同一个 5 分钟窗口，主机时钟漂移会让每条应答都被判成超窗，而错误消息里的
     「判定为重放」只是措辞 —— 先对时，别急着当成攻击。

非 2xx 的处理（有签名头就一定验，验不过就返回验签错误）：

| 应答 | 缺签名头时 |
| --- | --- |
| 2xx | **拒绝**（`VerifyError`）—— 包括关单的 204：空 body 的验签串是 `{时间戳}\n{随机串}\n\n`，微信照签 |
| 4xx | **拒绝**（`VerifyError`，消息里带状态码与原始 body）—— 这类应答的 `code` 会被当成业务结论，不能让它来自无法验证的来源 |
| 5xx | **放行**，但错误消息标注 `[未验签]`：5xx 不含可被误判的业务语义（伪造它只能诱发重试），而严格拒绝会把 CDN / 网关的 5xx 从「自动重试」变成「不重试」 |

> 残余风险：应答签名只覆盖 **时间戳 / 随机串 / 应答体**，**不绑定请求**，所以 5 分钟窗口内
> 抓到的真实应答理论上可被重放（对当前端点集，重放成功应答不会改变状态）。
> 这与回调的取舍一致：窗口 + 签名，不解决「网络层被完全接管」。

## 平台证书轮换

轮换期微信会**同时下发新旧两张都在有效期内**的平台证书，所以必须按请求头
`Wechatpay-Serial` 选键。写死单张证书会让轮换期的回调验签全部失败 —— 也就是订单不发货。

`PlatformKeys` 就是 `serial_no -> 公钥 PEM` 的索引（已过期的证书会被丢弃）：

- **出站应答验签**：索引由 SDK 自己维护 —— 首个请求前自动拉取，之后按 12 小时窗口刷新，
  遇到未知 serial 立即刷新一次后重验。想看当前认了哪几张：
  `wechat_pay.platform_keys().serials()`。
- **回调验签**：用你自己持有的索引（`fetch_platform_keys()` 每次返回**全新**的索引，
  调用方应整体替换而不是逐条合并），或在回调里取 `wechat_pay.platform_keys()` 快照。
  拿到 `UnknownPlatformSerial` 说明微信正在轮换证书：调
  `wechat_pay.refresh_platform_keys_for_unknown_serial(&serial)` 刷新后重验 ——
  ⚠ **不要**在回调里用不带限流的 `refresh_platform_keys()`：回调的 serial 是未鉴权输入，
  伪造一个就能触发刷新，那等于把证书接口（官方要求 12 小时一次）变成放大器。
  （`example/` 的回调实现就是这么写的。限流窗口只在刷新**成功**后计 —— 瞬时失败会尽快再试，
  不会把随后一分钟的验签全拖死。）
- 官方要求至少每 12 小时刷新一次；`PlatformKeys::needs_refresh(now)` 按
  `REFRESH_INTERVAL_SECS` 帮你判断。⚠ 用 `set_platform_keys` /
  `with_platform_public_key` 设置的索引属于**调用方负责**：SDK 不会自动拉取或替换它
  （否则会把公钥模式配置的公钥抹掉）。唯一的例外是 `add_static_platform_key`：它只写**静态表**，
  自动轮换照旧 —— 两张表并存时选键**静态表优先**，轮换表的安装只替换轮换表那一半。
  另外，已注入静态密钥且轮换表从未拉取过时 `needs_refresh` 返回 `false`（不预拉，
  改为遇到未知 serial 时按需拉一次）。
- 单飞刷新期间其它请求会等到刷新结束（默认最多 **2 秒**）；并发冷启动多、证书接口慢的场景
  可以用 `wechat_pay.with_key_refresh_wait(Duration::from_secs(5))` 调大预算 ——
  等超预算的请求会拿到 `UnknownPlatformSerial`（消息里带预算毫秒数），`Duration::ZERO`
  则是完全不等待。

## 交易账单（对账）

```rust
use wechat_pay_rust_sdk::model::{BillTarType, BillType};

// 1) 申请账单：微信每日 10 点后生成「昨日」账单（北京时间），只支持三个月内、
//    且账单里**只有支付成功的订单**、金额单位是**元**。
let bill = wechat_pay
    .trade_bill("2026-09-28", BillType::Success, Some(BillTarType::Gzip))
    .await?;

// 2) 下载账单文件（原始字节；GZIP 时是 gzip 流，自己解压）
let bytes = wechat_pay.download_bill(&bill.download_url).await?;

// 3) 官方要求：比对 hash_value 确认完整性（hash_type 固定 SHA1）
assert_eq!(sha1_hex(&content), bill.hash_value);
```

⚠ 与其它端点相反的两点（官方明文，别照抄「默认强制验签」那套）：

- **账单文件的应答不带签名头** ⇒ `download_bill` 跳过应答验签，也**不**声称内容已验签；
  完整性只能靠第 3 步的摘要比对。
- **下载地址是绝对的** ⇒ 按官方给的地址原样请求（不套用 `base_url`），且只接受 `/v3/` 之下、
  默认只接受 https。

错误处理：`NO_STATEMENT_EXIST`（该日没有账单）与 `STATEMENT_CREATING`（仍在生成）会以
`ApiError.response.code` 返回，调用方据此区分「没账单」与「稍后再取」。

## 错误处理

所有失败 —— 包括**非 2xx** 与 **HTTP 200 但 body 是错误信封** —— 都返回
`Err(PayError::ApiError)`。用 `kind()` 做三层归类决定处置策略：

```rust
use wechat_pay_rust_sdk::error::{ErrorKind, PayError};

match wechat_pay.jsapi_pay(params).await {
    Ok(response) => { /* response.prepay_id / response.sign_data */ }
    Err(err) => match err.kind() {
        // 传输层失败。重试判定由 SDK 的失败分类完成（见「自动重试」一节）：
        // 确定没送到的可安全重试；⚠ 结果未知的（读写超时）写接口不会自动重试，
        // 应当先用 query_order 确认状态。
        ErrorKind::Network => { /* … */ }
        // 微信业务拒绝：按 response.code 分支，不要重试。
        ErrorKind::Api => {
            if let PayError::ApiError { response, .. } = &err {
                eprintln!(
                    "code={:?} message={:?} detail={:?}",
                    response.code, response.message, response.detail
                );
            }
        }
        // 本地错误：签名、解密、JSON 解析、验签失败（含应答验签）、回调超窗 —— 通常要告警。
        ErrorKind::Local => { /* … */ }
    },
}
```

`response.detail` 是微信的字段级定位信息（例如 `/payer/openid`），
而 `Display` 会把它一起渲染出来 —— 日志外发前记得脱敏。

`ApiError` 还带 `request_id`（应答头 `Request-ID`，缺失为 `None`）—— 微信官方要求把它写进日志；
与微信侧排查（退款未受理、查单异常）时提供该值可快速定位到请求记录。取到该值时 `Display` 会附上
` (request-id: …)`。

⚠ **应答验签失败会改变这两层的边界**（默认开启）：无签名头的 4xx 会变成
`VerifyError`（`Local`）而不是 `ApiError` —— 此时**拿不到** `response.code`，
必须按「结果未知」处置（查单 / 告警），**不要**当成「订单不存在」。
无签名头的 5xx 仍是 `ApiError`，但消息里带 `[未验签]` 标记。详见「应答验签」一节。

## 超时与连接复用

HTTP 客户端由 `WechatPay` 持有并跨请求复用（连接池共享）。
默认超时 connect 5s / request 10s / pool idle 90s，要改：

```rust
use std::time::Duration;
use wechat_pay_rust_sdk::pay::HttpTimeouts;

let wechat_pay = wechat_pay.with_timeouts(HttpTimeouts {
    request: Duration::from_secs(30),
    ..HttpTimeouts::default()
});
```

⚠ **超时不代表操作没有发生**：请求很可能已被微信受理，只是响应没回来。
用 `PayError::may_have_taken_effect()` 判断 —— 它为 `true` 时不要当硬失败处理，
支付 / 退款应当先用 `query_order` / `query_refund` 确认最终状态。

## 自动重试

默认**开着**，但只重试「确定没送达」「微信明确说没受理」以及「微信已受理但没处理完」
这三类失败 —— 它们都有官方依据，而且重放的请求**字节完全一致**，所以
**默认策略下不会因为重试产生第二笔下单或第二笔退款**。

判据是一次失败在微信侧到底处于什么状态：

| 失败 | 典型场景 | 微信侧 | 只读接口 | 写 / 退款 |
| --- | --- | --- | --- | --- |
| 确定没送到 | 连接被拒、连接超时 | 没收到 | 重试 | **重试** |
| 明确未受理 | 429 / 500 / 502 / 503、`SYSTEM_ERROR` | 没受理 | 重试 | **重试** |
| 已受理、未处理完 | 202 | **已收到**，只是还没处理完 | 重试 | **重试** |
| 结果未知 | 读写超时、504 等官方未定义的 5xx | **可能已处理** | 重试 | **不重试** |
| 已处理 | 响应体没读完就断连 | 处理过了 | 不重试 | 不重试 |
| 应答验签失败 | 签名错 / 缺签名头 / 未知 serial / 超窗 | 应答**已收到** | 不重试 | 不重试 |

⚠ **应答验签失败不重试，但 `may_have_taken_effect()` 返回 `true`**：应答都完整回来了，
说明请求早已送达微信，而且微信会故意下发错误签名探测验签实现 —— 判成「确定没发生」会
诱导调用方换个单号重开。缺签名头的 5xx 是唯一例外：它被放行成 `ApiError`，因此照上表重试。

上表的措辞直接来自微信官方 HTTP 状态码页：429 是「**请求未受理**」，502/503 是
「**请求无法处理**」，各接口的 500 都写「**请用相同参数重新调用**」，202 则是
「服务器已接受请求，但尚未处理，**请使用原参数重复请求一遍**」。

⚠ **202 不等于「没发生」** —— 请求已经被微信接收，只是还没处理完，可能随后生效。
所以 `PayError::may_have_taken_effect()` 对 202 返回 `true`：拿到 `ApiError { status: 202 }`
时应当去查单确认，而**不是**换个单号重新下单。

「2xx + 错误信封」走同一条判定：微信会以 HTTP 200 返回 `{"code","message","detail"}`，
其中 `SYSTEM_ERROR` 与 HTTP 500 同源（官方要求「请用相同参数重新调用」），因此同样会重试；
其余错误码（`PARAM_ERROR` 之类）重试不会有不同结果，立即返回 `Err`。

**写接口超时为什么不重试**：微信以 `out_trade_no` / `out_refund_no` 作为订单与退款的
身份键，SDK 内部重放又是字节完全一致的，所以重放本身不会产生第二笔。但官方对超时的
口径是「先查单确认状态」，而不是「直接再发一次」：

```rust
match wechat_pay.jsapi_pay(params).await {
    Err(err) if err.may_have_taken_effect() => {
        // 结果未知：去查单，不要换个单号重新下单
        let status = wechat_pay.query_order(&out_trade_no).await?;
        // …
    }
    other => { /* … */ }
}
```

### 退避与次数

```rust
use std::time::Duration;
use wechat_pay_rust_sdk::retry::RetryPolicy;

// 通用：默认 3 次尝试，指数退避 + 全抖动
// （抖动是**向下**取的，所以首次等待是 0–200ms 之间的随机值，base_delay 是上限）
let wechat_pay = wechat_pay.with_retry(RetryPolicy {
    max_attempts: 5,
    ..RetryPolicy::default()
});
```

退款走**独立**的分钟级策略（最多 2 次尝试、首次退避 60s，且**刻意不加抖动**）—— 官方对
退款重试的要求是「间隔 1 分钟」，且该接口在失败时报错限流只有 6QPS，秒级退避打过去基本
是白打。这一档关掉抖动的原因也在于此：抖动会把「至少间隔 1 分钟」变成「可能立刻重发」。

```rust
// 同步链路扛不住分钟级阻塞时，关掉退款重试，改由业务侧异步重试
let wechat_pay = wechat_pay.with_refund_retry(RetryPolicy::disabled());
```

> ⚠ 默认值下的最坏耗时：通用接口约 `3 × 10s + 退避 ≈ 31s`；**退款约 80s**
> （请求超时 10s + 退避 60s + 重试 10s）。落在「用户在小程序里等」这种链路上时，
> 请显式调小次数，或用 `RetryPolicy::disabled()` 关掉。

「该不该重试」由失败分类决定，**策略只控制次数与退避** —— 把 `max_attempts` 调大
也不会让写接口在超时后被重试。

## 主备域名（跨城冗灾）

官方要求「正常使用主域名调用……当域名出现请求超时、读写超时，自动切换备域名重试」
（备域名 `api2.mch.weixin.qq.com`）。配置备域名后**每一次重试**都会轮换到下一个地址
（主 → 备 1 → 备 2 → 主 → …）：

```rust
let wechat_pay = wechat_pay.with_base_urls(
    "https://api.mch.weixin.qq.com",
    ["https://api2.mch.weixin.qq.com"],
);
```

- host **不参与签名**（签名串第二行只有 path + query），换域名重发与在同一域名上重试的
  重放语义完全一致；「该不该重试」仍然只由失败分类决定。
- ⚠ 写接口「结果未知」的失败（读写超时）**不会**重试 —— 不存在「超时后换个域名重发写请求」
  这种越界行为，它依然要由你先查单确认。
- 不配置备域名（默认）时行为与以前完全一致。
- ⚠ 这两个地址只能来自你自己的配置，**绝不能让用户输入接触它们**。