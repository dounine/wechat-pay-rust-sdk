//! 微信支付回调服务器（actix-web 参考实现）。
//!
//! 只演示**回调**这一条链路：**验签 → 解密 → 幂等落库 → 应答**。
//! 下单在业务流程里做（小程序/JSAPI 用 `jsapi_pay`），不在回调服务器里演示。
//!
//! # 为什么这几步缺一不可
//!
//! | 少做哪步 | 后果 |
//! | --- | --- |
//! | 不验签 | 任何人都能伪造「支付成功」通知让你发货 |
//! | 不查时间戳 | 抓到一次合法回调即可无限重放 —— 发一次货，重复领取 |
//! | 写死单张证书 | 平台证书轮换期验签全部失败 → 回调被拒 → 订单不发货 |
//! | 不做幂等 | 微信重投 15 次 → 重复发货 |
//!
//! 环境变量见 [`WechatPay::from_env`]：`WECHAT_APPID` / `WECHAT_MCH_ID` /
//! `WECHAT_PRIVATE_KEY`（**PEM 文件路径**，不是密钥内容）/ `WECHAT_SERIAL_NO` /
//! `WECHAT_V3_KEY`（32 字节）/ `WECHAT_NOTIFY_URL`。

use actix_web::web::{Bytes, Data};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, post};
use wechat_pay_rust_sdk::error::PayError;
use wechat_pay_rust_sdk::model::{WechatPayDecodeData, WechatPayNotify};
use wechat_pay_rust_sdk::notify::NotifyHeaders;
use wechat_pay_rust_sdk::pay::{PayNotifyTrait, WechatPay};

/// 失败应答：官方要求校验或处理失败时返回 4XX / 5XX，body 为
/// `{"code":"FAIL","message":"…"}`，微信收到后会按退避策略重投。
fn fail(message: impl std::fmt::Display) -> HttpResponse {
    HttpResponse::BadRequest().json(serde_json::json!({
        "code": "FAIL",
        "message": message.to_string(),
    }))
}

/// 支付结果回调。
///
/// ⚠ 官方要求 **5 秒内**应答，所以真实的落库/发货必须异步化，不要在这个函数里同步做完。
#[post("/pay/notify")]
async fn pay_notify(req: HttpRequest, body: Bytes, wechat_pay: Data<WechatPay>) -> HttpResponse {
    // ① 提取四个验签请求头（缺任意一个都会返回 Err）。
    let headers = match NotifyHeaders::from_pairs(
        req.headers()
            .iter()
            .map(|(name, value)| (name.as_str(), value.to_str().unwrap_or_default())),
    ) {
        Ok(headers) => headers,
        Err(err) => return fail(err),
    };

    // ⚠ 必须拿**原始** body 验签：先反序列化再序列化会改变字节，验签必然失败。
    let raw_body = match std::str::from_utf8(&body) {
        Ok(raw_body) => raw_body,
        Err(_) => return fail("回调 body 不是合法 UTF-8"),
    };

    // ② 验签三步：时间戳新鲜度（±300s）→ 按 Wechatpay-Serial 选键 → 验签。
    if let Err(err) = verify_notify_with_refresh(&wechat_pay, &headers, raw_body).await {
        return fail(err);
    }

    // ③ 解密 resource（decrypt_notify 会先校验 resource.algorithm）。
    //    解密不能替代验签 —— 顺序永远是先验签再解密。
    let notify: WechatPayNotify = match serde_json::from_str(raw_body) {
        Ok(notify) => notify,
        Err(err) => return fail(format!("回调 JSON 解析失败: {err}")),
    };
    let data: WechatPayDecodeData = match wechat_pay.decrypt_notify(&notify.resource) {
        Ok(data) => data,
        Err(err) => return fail(err),
    };

    // ④ 幂等落库 —— **SDK 不代劳这一步**。
    //    微信在收到成功应答前会重投（最多 15 次），必须按 out_trade_no /
    //    transaction_id 去重，且重复投递要返回**成功**，否则微信会一直重投。
    handle_payment(&data);

    // ⑤ 成功应答：200 或 204，且**不带 body**。
    HttpResponse::NoContent().finish()
}

/// 回调验签，含证书轮换兜底。
///
/// 索引取的是客户端当前的快照（`WechatPay` 自己维护并按 12 小时刷新）；
/// 拿到 `UnknownPlatformSerial` 说明微信正在轮换平台证书 —— **立即重新拉取**再验一次，
/// 不要拿别的密钥去试。
///
/// ⚠ 这里必须用 `refresh_platform_keys_for_unknown_serial` 而不是
/// `refresh_platform_keys`：回调里的 `Wechatpay-Serial` 是**未鉴权输入**，伪造一个就能
/// 触发刷新；那个入口带 60s 最小间隔（与 SDK 内部对出站应答的处理一致），
/// 无节流等于把 `/v3/certificates` 变成可被外部触发的放大器。
///
/// ⚠ 出站请求的应答验签由 SDK 自己做（默认强制，含同一条轮换兜底路径），
/// 这里处理的是**回调**方向，两者不共用调用点。
async fn verify_notify_with_refresh(
    wechat_pay: &WechatPay,
    headers: &NotifyHeaders,
    raw_body: &str,
) -> Result<(), PayError> {
    match wechat_pay.platform_keys().verify_notify(headers, raw_body) {
        Err(PayError::UnknownPlatformSerial(serial)) => {
            wechat_pay
                .refresh_platform_keys_for_unknown_serial(&serial)
                .await?;
            wechat_pay.platform_keys().verify_notify(headers, raw_body)
        }
        // StaleNotify = 疑似重放，直接拒绝并告警；其余错误原样交给调用方。
        other => other,
    }
}

/// 业务落库的占位实现 —— 真实项目请换成「幂等写库 + 异步发货」。
fn handle_payment(data: &WechatPayDecodeData) {
    tracing::info!(
        out_trade_no = %data.out_trade_no,
        transaction_id = %data.transaction_id,
        trade_state = %data.trade_state,
        "收到支付通知（请在此处按 out_trade_no 幂等落库）"
    );
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // 启动时预热平台证书：`refresh_platform_keys` 会把索引装进客户端（出站应答验签
    // 用的就是它），同时返回一份快照 —— 用来记日志、以及让凭证错误在启动时就暴露。
    // 之后 SDK 自己按 12 小时窗口刷新，并在遇到未知 `Wechatpay-Serial` 时刷新后重验。
    let bootstrap = WechatPay::from_env();
    let keys = bootstrap
        .refresh_platform_keys()
        .await
        .expect("拉取平台证书失败：检查商户证书、证书序列号与 APIv3 密钥");
    tracing::info!(count = keys.len(), serials = ?keys.serials(), "已加载平台密钥");

    // `WechatPay` 不实现 Clone（它持有连接池），所以每个 worker 各建一份 ——
    // 各 worker 会在自己的首个请求前各自完成一次冷启动拉取。
    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(WechatPay::from_env()))
            .service(pay_notify)
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
