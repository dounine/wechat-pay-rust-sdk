use actix_web::{App, get, HttpRequest, HttpResponse, HttpServer, post, Responder};
use actix_web::web::{Bytes, get, Json, JsonConfig};
use dotenvy::dotenv;
use tracing::debug;
use wechat_pay_rust_sdk::model::{WechatPayDecodeData, WechatPayNotify};
use wechat_pay_rust_sdk::pay::{PayNotifyTrait, WechatPay};

#[post("/pay/notify")]
async fn pay_notify(bytes: Bytes, req: HttpRequest) -> impl Responder {
    let headers = req.headers();
    headers.iter().for_each(|(k, v)| {
        debug!("{}: {:?}", k, v);
    });
    let str = String::from_utf8(bytes.to_vec()).unwrap();
    std::fs::write("body.txt", bytes).unwrap();
    debug!("body: {}", str);
    HttpResponse::Ok().json(serde_json::json!({
        "code": "SUCCESS",
        "message": "成功"
    }))
}

#[post("/pay/notify3")]
async fn pay_notify3(bytes: Bytes, req: HttpRequest) -> impl Responder {
    let headers = req.headers();
    let wechatpay_signatrue = headers.get("wechatpay-signature").unwrap().to_str().unwrap();
    let wechatpay_timestamp = headers.get("wechatpay-timestamp").unwrap().to_str().unwrap();
    let wechatpay_nonce = headers.get("wechatpay-nonce").unwrap().to_str().unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let wechat_pay = WechatPay::from_env();
    let pub_key = std::fs::read_to_string("pubkey.pem").unwrap();
    let body = format!("{}\n{}\n{}\n", wechatpay_timestamp, wechatpay_nonce, body);
    wechat_pay.verify_signatrue(
        pub_key.as_str(),
        wechatpay_timestamp,
        wechatpay_nonce,
        wechatpay_signatrue,
        body.as_str(),
    ).expect("签名验证失败，非法数据");
    HttpResponse::Ok().json(serde_json::json!({
        "code": "SUCCESS",
        "message": "成功"
    }))
}

#[get("/")]
async fn home() -> impl Responder {
    HttpResponse::Ok().body("hello rust")
}

#[post("/pay/notify2")]
async fn pay_notify2(bytes: Bytes, data: Json<WechatPayNotify>, req: HttpRequest) -> impl Responder {
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    debug!("body: {}", body);
    req.headers().iter().for_each(|(k, v)| {
        debug!("{}: {:?}", k, v);
    });
    let data = data.into_inner();
    debug!("data: {:#?}", data);
    let nonce = data.resource.nonce;
    let ciphertext = data.resource.ciphertext;
    let associated_data = data.resource.associated_data.unwrap_or_default();
    dotenv().ok();
    let wechat_pay = WechatPay::from_env();
    let result: WechatPayDecodeData = wechat_pay.decrypt_paydata(
        ciphertext,
        nonce,
        associated_data,
    ).unwrap();
    debug!("result: {:#?}", result);
    HttpResponse::Ok().json(serde_json::json!({
        "code": "SUCCESS",
        "message": "成功"
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_line_number(true)
        .init();
    HttpServer::new(move || {
        App::new()
            .service(pay_notify)
            .service(pay_notify2)
            .service(home)
    })
        .bind(("0.0.0.0", 8080))?
        .workers(1)
        .run()
        .await
}
