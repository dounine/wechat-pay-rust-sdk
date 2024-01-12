use actix_web::{App, get, HttpRequest, HttpResponse, HttpServer, post, Responder};
use actix_web::web::{Bytes, Json, JsonConfig};
use dotenvy::dotenv;
use tracing::debug;
use wechat_pay_rust_sdk::model::{WechatPayDecodeData, WechatPayNotify};
use wechat_pay_rust_sdk::pay::{PayNotifyTrait, WechatPay};

#[post("/pay/notify")]
async fn pay_notify(data: Json<WechatPayNotify>, req: HttpRequest) -> impl Responder {
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
    })
        .bind(("0.0.0.0", 8080))?
        .workers(1)
        .run()
        .await
}
