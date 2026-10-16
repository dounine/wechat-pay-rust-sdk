use crate::error::PayError;
use crate::macros::debug;
use crate::model::AppParams;
use crate::model::H5Params;
use crate::model::JsapiParams;
use crate::model::MicroParams;
use crate::model::NativeParams;
use crate::model::ParamsTrait;
use crate::model::RefundsParams;
use crate::model::{BillTarType, BillType};
use crate::notify::{NotifyHeaders, check_timestamp_skew, response_signature_headers};
use crate::pay::{PackagePrefix, ResponseVerify, WechatPay, WechatPayTrait};
use crate::request::HttpMethod;
use crate::response::AppResponse;
use crate::response::H5Response;
use crate::response::JsapiResponse;
use crate::response::MicroResponse;
use crate::response::RefundsResponse;
use crate::response::ResponseTrait;
use crate::response::{
    CertificateResponse, NativeResponse, TradeBillResponse, TransactionResponse,
};
use crate::retry::{Delivery, RequestKind, classify, should_retry};
use reqwest::header::{HeaderMap, REFERER};
use serde_json::{Map, Value};
use std::time::Duration;

#[cfg(feature = "async")]
use reqwest::RequestBuilder;
#[cfg(not(feature = "async"))]
use reqwest::blocking::RequestBuilder;

#[cfg(feature = "async")]
use maybe_async::maybe_async as maybe_async_attr;
#[cfg(not(feature = "async"))]
use maybe_async::must_be_sync as maybe_async_attr;

/// 原始应答：状态码 + 响应头 + **原始字节** body。
///
/// 应答验签必须用这里的原始字节（验签串的第三行就是 body 本身）—— 一旦先转成
/// `String` 再序列化，字节就变了，验签必然失败。用 `Vec<u8>` 而不是 `reqwest` 内部的
/// `Bytes`：后者没有从 `reqwest` 再导出，命名它就得把 `bytes` 加进依赖，而这里拷一次
/// 小体积 JSON 的成本远低于多一个依赖。
pub(crate) struct RawResponse {
    status: u16,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
}

/// 应答验签的调用方式。**没有默认值**：每个新端点都必须显式选一个。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseCheck {
    /// 普通端点：按「2xx 必须带签名头 / 4xx 无头即错 / 5xx 无头放行并标记」处理。
    Strict,
    /// `GET /v3/certificates`：应答本身就是密钥的来源，由调用方在解密后**自校验**。
    CertificateSelfCheck,
}

/// 验签结果（决定非 2xx 的错误消息要不要打 `[未验签]` 标记）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignatureStatus {
    /// 已按规则验签通过；或调用方显式关闭了验签。
    Verified,
    /// 没验签：5xx 缺签名头（放行但必须标记），或证书列表路径上的非 2xx 应答
    /// （那条路径的 2xx 由调用方自校验，错误体的签名我们一概没验过）。
    Unsigned,
}

impl SignatureStatus {
    /// 未走验签的路径（`ResponseCheck::CertificateSelfCheck`）上的状态。
    ///
    /// 那条路径的 **2xx** 由调用方拿响应内下发的证书自校验；**非 2xx** 的错误体
    /// 则从头到尾没验过 —— 标出来，免得调用方以为那条错误是可信的。
    fn for_self_checked(status: u16) -> Self {
        if (200..300).contains(&status) {
            SignatureStatus::Verified
        } else {
            SignatureStatus::Unsigned
        }
    }
}

/// 发送请求并取回**未经处理**的应答。
///
/// 微信支付在失败时返回**非 2xx 状态码** + `{"code","message","detail"}` 响应体，
/// 而成功响应类型字段全是 `Option`，能把它照单全收 —— 所以调用方必须先看状态码再看
/// body。本函数**不做**任何业务判断：非 2xx 的错误构造、错误信封识别与应答验签都在
/// [`WechatPay::request`] 里按「先验签、后归一」的顺序做（验签必须优先于业务错误，
/// 否则伪造的 4xx 信封会被当成微信的业务拒绝）。
#[maybe_async_attr]
async fn send_and_check(builder: RequestBuilder) -> Result<RawResponse, PayError> {
    let response = builder.send().await?;
    let status = response.status().as_u16();
    // 头要先取出来：`bytes()` 会消费掉 response。
    let headers = response.headers().clone();
    let body = response.bytes().await?.to_vec();
    debug!(
        "status: {} request-id: {} body: {}",
        status,
        (request_id_of(&headers).as_deref().unwrap_or("-")),
        (String::from_utf8_lossy(&body))
    );
    Ok(RawResponse {
        status,
        headers,
        body,
    })
}

/// 重试前的退避等待。
///
/// 同步模式阻塞当前线程；异步模式让出执行权（依赖 `tokio` 的 `time`）。
/// 两份定义按 feature 择一编译，同步版本**刻意不是 `async fn`** ——
/// 否则调用方在同步模式下拿到的是一个永不轮询的 Future，睡不成。
///
/// `pub(crate)`：平台证书刷新的单飞等待（`src/cert.rs`）也用这一份。
#[cfg(feature = "async")]
pub(crate) async fn sleep_for(delay: Duration) {
    tokio::time::sleep(delay).await;
}

/// 重试前的退避等待（同步实现，见异步版本的说明）。
#[cfg(not(feature = "async"))]
pub(crate) fn sleep_for(delay: Duration) {
    std::thread::sleep(delay);
}

/// 只取顶层 `code`，用于识别「状态码是 2xx、body 却是错误信封」。
#[derive(serde::Deserialize)]
struct EnvelopeProbe {
    code: Option<serde_json::Value>,
}

/// 2xx 的响应体是否是微信的错误信封。
///
/// 微信会以 200 返回 `{"code": "...", "message": "..."}`，而成功响应类型字段全是
/// `Option`，能把它照单全收 —— 于是失败**再一次**伪装成成功。这是单靠状态码检查
/// 覆盖不到的那一半。
///
/// 判据：顶层出现**非空字符串** `code`。本 crate 解析的成功响应
/// （`prepay_id` / `code_url` / `h5_url` / `data` / 交易对象）都不含顶层 `code`，
/// 所以不会误伤。解析失败（非 JSON、数组、非法 UTF-8）一律视为不是信封。
///
/// 先用原始字节解析（与验签用的是同一份数据），失败再退回**有损文本**：
/// 改动前这条判据吃的是 `response.text()`（本身就有损替换），只吃字节会让
/// 「非 UTF-8 的 2xx 错误信封」漏判 —— 关单那条路（`request_no_content` 不看响应体、
/// 也不解析 JSON）会把它当成功返回。判据不参与验签，用有损文本没有安全影响。
fn is_error_envelope(body: &[u8]) -> bool {
    let parsed = serde_json::from_slice::<EnvelopeProbe>(body)
        .or_else(|_| serde_json::from_str::<EnvelopeProbe>(&String::from_utf8_lossy(body)));
    match parsed {
        Ok(probe) => probe
            .code
            .as_ref()
            .and_then(|value| value.as_str())
            .is_some_and(|code| !code.is_empty()),
        Err(_) => false,
    }
}

/// 取应答头 `Request-ID`（微信的请求唯一标识）。头名大小写不敏感；非 UTF-8 视为缺失。
fn request_id_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get("request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

impl WechatPay {
    /// 底层请求：签名 → 发送 → **应答验签** → 错误归一，失败时按策略重试。
    ///
    /// **不注入任何字段** —— `appid` / `mchid` / `notify_url` 的注入只发生在 `pay()` 里。
    /// 这是有意的：关单只需要 `mchid`、查单的 `mchid` 要放在 query string，
    /// 无脑注入这三个字段会让这些端点直接失败。
    ///
    /// ⚠ 传入的 `url` 会**原样参与签名**，所以 GET 的查询串必须拼进 `url`
    /// （微信要求签名串第二行是 path + `?` + query）。
    ///
    /// # 顺序（不可调换）
    ///
    /// 1. **验签优先**：能用签名头判定时，验签失败先于业务错误返回。否则能改应答的
    ///    中间层可以伪造 4xx 信封，把「来源无法验证」伪装成微信的业务拒绝。
    /// 2. **归一**：非 2xx → [`PayError::ApiError`]（无签名头的 5xx 带 `[未验签]` 标记）；
    ///    2xx + 错误信封 → [`PayError::ApiError`]。
    /// 3. **重试判定**（见下）。
    ///
    /// # 重试
    ///
    /// **「能不能重试」由 `kind`（重放语义）与失败分类共同决定，策略只控制次数与退避**
    /// —— 详见 [`crate::retry`]。核心是这几条：
    ///
    /// - 确定没送到（连接失败）或微信明确未受理（429 / 500 / 502 / 503 / `SYSTEM_ERROR`）
    ///   → 任何接口都可重试
    /// - 202（已受理、尚未处理完）→ 任何接口都可重试，但它**不等于没发生**
    /// - 结果未知（读写超时、官方未定义的 5xx）→ **只有只读接口重试**；
    ///   写接口交给调用方查单确认
    /// - 验签失败 → 不重试（但**可能已生效**，见 [`PayError::may_have_taken_effect`]）
    ///
    /// 每次尝试都重新签名：重试可能跨过 5 分钟的签名有效窗口，复用旧签名会直接 401。
    /// 每次尝试也各自验签（应答是新的，签名自然也是新的）。
    ///
    /// 每次尝试的网关地址按
    /// [`WechatPay::with_base_urls`](crate::pay::WechatPay::with_base_urls) 轮换
    /// （未配置备域名时恒为主域名）。
    #[maybe_async_attr]
    pub(crate) async fn request(
        &self,
        method: HttpMethod,
        url: &str,
        body: &str,
        kind: RequestKind,
        check: ResponseCheck,
    ) -> Result<RawResponse, PayError> {
        let policy = self.policy_for(kind);
        let max_attempts = policy.max_attempts();
        let mut attempt: u32 = 1;

        loop {
            let headers = self.build_header(method, url, body)?;
            // 复用 `WechatPay` 持有的客户端：连接池跨请求共享，不必每次重新建连 / TLS 握手。
            let client = &self.client;
            let full_url = format!("{}{}", self.gateway_for_attempt(attempt), url);
            debug!("url: {} body: {}", full_url, body);
            let builder = match method {
                HttpMethod::GET => client.get(full_url),
                HttpMethod::POST => client.post(full_url),
                HttpMethod::PUT => client.put(full_url),
                HttpMethod::DELETE => client.delete(full_url),
                HttpMethod::PATCH => client.patch(full_url),
            };

            let outcome = send_and_check(builder.headers(headers).body(body.to_owned())).await;

            // ① 验签优先（见上面的顺序说明）。
            let outcome = match outcome {
                Ok(raw) if check == ResponseCheck::Strict => match self.verify_response(&raw).await
                {
                    Ok(signature) => Ok((raw, signature)),
                    Err(err) => Err(err),
                },
                // 证书列表的应答是密钥来源，由调用方解密后自校验。
                Ok(raw) => {
                    let signature = SignatureStatus::for_self_checked(raw.status);
                    Ok((raw, signature))
                }
                Err(err) => Err(err),
            };

            // ② 归一：非 2xx 与「2xx + 错误信封」都变成 `Err`，于是 `SYSTEM_ERROR`
            //    （与 HTTP 500 同源，官方要求「请用相同参数重新调用」）会被重试；
            //    关单也不会再把「HTTP 200 + 错误信封」当成成功（它原先根本不看响应体）。
            let outcome = match outcome {
                Ok((raw, signature)) if !(200..300).contains(&raw.status) => {
                    let text = String::from_utf8_lossy(&raw.body);
                    Err(match signature {
                        SignatureStatus::Unsigned => PayError::api_error_unverified(
                            raw.status,
                            &text,
                            request_id_of(&raw.headers),
                        ),
                        SignatureStatus::Verified => {
                            PayError::api_error(raw.status, &text, request_id_of(&raw.headers))
                        }
                    })
                }
                Ok((raw, _)) if is_error_envelope(&raw.body) => Err(PayError::api_error(
                    raw.status,
                    &String::from_utf8_lossy(&raw.body),
                    request_id_of(&raw.headers),
                )),
                other => other.map(|(raw, _)| raw),
            };

            let delivery = match &outcome {
                // 202 是「已受理但尚未处理」，官方要求「请使用原参数重复请求一遍」：
                // 照样重试，但它**不等于没发生**（见 `Delivery::Accepted`）。
                Ok(raw) if raw.status == 202 => Some(Delivery::Accepted),
                Ok(_) => None,
                Err(err) => classify(err),
            };
            let Some(delivery) = delivery else {
                return outcome;
            };

            if !should_retry(delivery, kind) || attempt >= max_attempts {
                // 重试用尽（或本就不该重试）。到这里还可能是 `Ok` 的只剩 202 ——
                // 它意味着「微信收了但没处理完」，必须降级成错误返回，
                // 否则下游会拿空 body 去解析，报出一个与真实原因无关的 JSON 错误。
                return match outcome {
                    Ok(raw) => Err(PayError::api_error(
                        raw.status,
                        &String::from_utf8_lossy(&raw.body),
                        request_id_of(&raw.headers),
                    )),
                    Err(err) => Err(err),
                };
            }

            let delay = policy.delay_for(attempt);
            debug!(
                "retry {}/{} after {:?} ({:?})",
                attempt, max_attempts, delay, delivery
            );
            attempt += 1;
            sleep_for(delay).await;
        }
    }

    /// 校验应答签名：2xx 必须带签名头，4xx 无头即错，5xx 无头放行但标记。
    ///
    /// 5xx 放行的理由：这类应答不含可被对手用来误判业务的语义（伪造 5xx 只能诱发重试，
    /// 与丢包等价），而把它变成 `VerifyError` 会让 CDN / 网关的 5xx 从「自动重试」变成
    /// 「不重试」—— 正好抹掉本 crate 的重试设计。4xx 则相反：伪造 `ORDER_NOT_EXIST`
    /// 能让调用方以为订单不存在，这是会被当成业务结论的语义。
    #[maybe_async_attr]
    async fn verify_response(&self, raw: &RawResponse) -> Result<SignatureStatus, PayError> {
        if self.response_verify == ResponseVerify::Disabled {
            return Ok(SignatureStatus::Verified);
        }
        // 密钥由 `request()` 在**发送前**确保（见那里的说明），这里只负责验签。
        let headers = match response_signature_headers(&raw.headers) {
            Ok(headers) => headers,
            Err(missing) => {
                if (500..600).contains(&raw.status) {
                    return Ok(SignatureStatus::Unsigned);
                }
                // 官方文档承认代理 / CDN 会过滤 `Wechatpay-*` 头（建议改代理或直连），
                // 所以错误里必须带上状态码与原文，否则用户拿不到任何排查线索。
                let body = String::from_utf8_lossy(&raw.body);
                return Err(PayError::VerifyError(format!(
                    "应答缺少签名头 {}（HTTP {}）—— 若链路上有代理 / CDN，\
                     请检查它们是否过滤了 Wechatpay-* 头；应答原文: {}",
                    missing.join(" / "),
                    raw.status,
                    crate::error::truncate_raw_body(&body),
                )));
            }
        };

        check_timestamp_skew(&headers.timestamp, crate::util::now_unix_secs())?;
        let body = std::str::from_utf8(&raw.body).map_err(|e| {
            PayError::VerifyError(format!("应答不是合法 UTF-8，无法按原始字节验签: {e}"))
        })?;
        self.verify_signed(&headers, body).await?;
        Ok(SignatureStatus::Verified)
    }

    /// 用索引里的公钥验签；未知 serial 时刷新平台证书**一次**后**重验**。
    ///
    /// 重验用的是已经缓冲在内存里的原始应答 —— **绝不重发业务请求**：对写接口重发会
    /// 改变重放语义（那是 `retry` 模块的管辖范围）。
    ///
    /// 「微信支付公钥」（`PUB_KEY_ID_…`）这类拉取也认识不了的 serial 由
    /// [`WechatPay::refresh_platform_keys_for_unknown_serial`] 直接给出可操作的错误，不在这里判。
    #[maybe_async_attr]
    async fn verify_signed(&self, headers: &NotifyHeaders, body: &str) -> Result<(), PayError> {
        match self.verify_against_index(headers, body) {
            Err(PayError::UnknownPlatformSerial(serial)) => {
                self.refresh_platform_keys_for_unknown_serial(&serial)
                    .await?;
                self.verify_against_index(headers, body)
            }
            other => other,
        }
    }

    /// 查表 + 一次 RSA 验签。锁内不做别的事，**不跨 `.await` 持锁**。
    fn verify_against_index(&self, headers: &NotifyHeaders, body: &str) -> Result<(), PayError> {
        self.platform_keys_read()
            .verify(
                &headers.serial,
                &headers.timestamp,
                &headers.nonce,
                body,
                &headers.signature,
            )
            // 签名不是合法 base64 也是「应答已收到、但没验过」：统一成 `VerifyError`，
            // 免得调用方从 `DecodeError` 拿到「确定没生效」这个错误结论
            // （官方探测流量长得就可能是这样：`WECHATPAY/SIGNTEST/` 后面接一段非 base64）。
            .map_err(|err| match err {
                PayError::DecodeError(error) => {
                    PayError::VerifyError(format!("应答签名不是合法 base64: {error}"))
                }
                other => other,
            })
    }

    /// `request` + JSON 解析。
    ///
    /// 「2xx + 错误信封」在 [`WechatPay::request`] 里就已经归一成
    /// [`PayError::ApiError`] 了（只有那样它才能参与重试判定），所以这里拿到的
    /// 一定是真正的成功响应体。
    ///
    /// 解析走 `from_slice`（原始字节）而不是先转 `String`：非法 UTF-8 的 2xx 响应体
    /// 会在这里报 [`PayError::JsonError`]，而不是被有损替换成 U+FFFD 后「解析成功」。
    #[maybe_async_attr]
    async fn request_json<R: ResponseTrait>(
        &self,
        method: HttpMethod,
        url: &str,
        body: &str,
        kind: RequestKind,
    ) -> Result<R, PayError> {
        self.ensure_keys().await?;
        let raw = self
            .request(method, url, body, kind, ResponseCheck::Strict)
            .await?;
        Ok(serde_json::from_slice::<R>(&raw.body)?)
    }

    /// `request` + 丢弃响应体，用于 **204 No Content**（例如关单）。
    ///
    /// 仍然要验签：空 body 的验签串是 `{timestamp}\n{nonce}\n\n`，微信照签
    /// （官方文档明确提到 204 这种「应答报文主体为空」的情形）。
    #[maybe_async_attr]
    async fn request_no_content(
        &self,
        method: HttpMethod,
        url: &str,
        body: &str,
        kind: RequestKind,
    ) -> Result<(), PayError> {
        self.ensure_keys().await?;
        self.request(method, url, body, kind, ResponseCheck::Strict)
            .await?;
        Ok(())
    }

    /// 通用下单：把 `params` 序列化后注入 `appid` / `mchid` / `notify_url` 再签名发送。
    ///
    /// 字段注入**只发生在这里** —— 其余端点走 `WechatPay::request_json`，不注入任何字段
    /// （关单的 body 只要 `mchid`，查单的 `mchid` 在 query string 里）。
    #[maybe_async_attr]
    pub async fn pay<P: ParamsTrait, R: ResponseTrait>(
        &self,
        method: HttpMethod,
        url: &str,
        json: P,
    ) -> Result<R, PayError> {
        let json_str = json.to_json()?;
        debug!("json_str: {}", json_str);
        let mut map: Map<String, Value> = serde_json::from_str(&json_str)?;
        map.insert("appid".to_owned(), self.appid().into());
        map.insert("mchid".to_owned(), self.mch_id().into());
        map.insert("notify_url".to_owned(), self.notify_url().into());
        let body = serde_json::to_string(&map)?;
        self.request_json(method, url, &body, RequestKind::Write)
            .await
    }

    /// 通用 GET：签名后请求 `url`，把响应解析成 `R`。
    ///
    /// ⚠ `url` 会**原样参与签名**，带查询参数时必须把查询串一起传进来。
    #[maybe_async_attr]
    pub async fn get_pay<R: ResponseTrait>(&self, url: &str) -> Result<R, PayError> {
        self.request_json(HttpMethod::GET, url, "", RequestKind::Read)
            .await
    }

    /// H5 支付（外部浏览器）：返回拉起微信收银台的 `h5_url`。
    #[maybe_async_attr]
    pub async fn h5_pay(&self, params: H5Params) -> Result<H5Response, PayError> {
        let url = "/v3/pay/transactions/h5";
        self.pay(HttpMethod::POST, url, params).await
    }
    /// APP 支付：返回 `prepay_id` 与给客户端拉起支付用的签名数据。
    ///
    /// ⚠ 签名数据的 `package` 前缀与其他支付方式不同，见 [`WechatPayTrait::mut_sign_data`]。
    #[maybe_async_attr]
    pub async fn app_pay(&self, params: AppParams) -> Result<AppResponse, PayError> {
        let url = "/v3/pay/transactions/app";
        let mut result: AppResponse = self.pay(HttpMethod::POST, url, params).await?;
        result.sign_data = match result.prepay_id.as_deref() {
            Some(prepay_id) => Some(self.mut_sign_data(PackagePrefix::Bare, prepay_id)?),
            None => None,
        };
        Ok(result)
    }
    /// JSAPI 支付（小程序 / 公众号）：返回 `prepay_id` 与给 `wx.requestPayment` 的签名数据。
    #[maybe_async_attr]
    pub async fn jsapi_pay(&self, params: JsapiParams) -> Result<JsapiResponse, PayError> {
        let url = "/v3/pay/transactions/jsapi";
        let mut result: JsapiResponse = self.pay(HttpMethod::POST, url, params).await?;
        result.sign_data = match result.prepay_id.as_deref() {
            Some(prepay_id) => Some(self.mut_sign_data(PackagePrefix::PrepayId, prepay_id)?),
            None => None,
        };
        Ok(result)
    }
    /// 付款码支付：返回 `prepay_id` 与签名数据。
    #[maybe_async_attr]
    pub async fn micro_pay(&self, params: MicroParams) -> Result<MicroResponse, PayError> {
        let url = "/v3/pay/transactions/jsapi";
        let mut result: MicroResponse = self.pay(HttpMethod::POST, url, params).await?;
        result.sign_data = match result.prepay_id.as_deref() {
            Some(prepay_id) => Some(self.mut_sign_data(PackagePrefix::PrepayId, prepay_id)?),
            None => None,
        };
        Ok(result)
    }
    /// 扫码支付：返回 `code_url`，由商户自行生成二维码。
    #[maybe_async_attr]
    pub async fn native_pay(&self, params: NativeParams) -> Result<NativeResponse, PayError> {
        let url = "/v3/pay/transactions/native";
        self.pay(HttpMethod::POST, url, params).await
    }

    /// 获取平台证书列表（`GET /v3/certificates`）。
    ///
    /// ⚠ 返回的证书是**加密**的，需要解密后才能用。要做回调验签请直接用
    /// [`WechatPay::fetch_platform_keys`]，它会把解密与建索引一次做完。
    ///
    /// 应答按 **R5 自校验**（用响应内下发的那张证书验它自己）：证书列表本身就是密钥来源，
    /// 若按本地索引严格验签，轮换期（应答由本地还不认识的新证书签名）就会失败 ——
    /// 静态密钥模式下更是必然失败。列表为空时没有密钥可供自证，直接放行（不含任何可误信的内容）。
    #[maybe_async_attr]
    pub async fn certificates(&self) -> Result<CertificateResponse, PayError> {
        let raw = self
            .request(
                HttpMethod::GET,
                "/v3/certificates",
                "",
                RequestKind::Read,
                ResponseCheck::CertificateSelfCheck,
            )
            .await?;
        let response: CertificateResponse = serde_json::from_slice(&raw.body)?;
        if self.response_verify != ResponseVerify::Disabled {
            let keys = self.build_keys_from_response(&response)?;
            if !keys.is_empty() {
                self.self_check_certificate_response(&raw.headers, &raw.body, &keys)?;
            }
        }
        Ok(response)
    }
    /// 从 H5 支付返回的页面里抓出 `weixin://` 拉起链接。
    ///
    /// ⚠ 实现是**逐行字符串扫描**（找包含 `weixin://` 的行再按 `"` 切分），
    /// 微信改页面结构就会失效 —— 生产环境建议前端自行处理 `h5_url`。
    /// ⚠ `h5_url` 会被直接 GET，且**没有白名单**：只能传微信返回的 `h5_url`
    /// 或你自己服务端的地址，**绝不能来自用户输入**（SSRF）。
    #[maybe_async_attr]
    pub async fn get_weixin<S>(&self, h5_url: S, referer: S) -> Result<String, PayError>
    where
        S: AsRef<str>,
    {
        // 同样复用共享客户端（目标是与微信 API 不同的主机，连接池不会命中，
        // 但仍比每次新建客户端好；且自动继承统一的超时配置）。
        let client = &self.client;
        let mut headers = HeaderMap::new();
        let referer = reqwest::header::HeaderValue::from_str(referer.as_ref())
            .map_err(|e| PayError::VerifyError(format!("非法 Referer: {e}")))?;
        headers.insert(REFERER, referer);
        let response = client.get(h5_url.as_ref()).headers(headers).send().await?;
        // `text()` 会消费 response，request-id 要在它之前取。
        let request_id = request_id_of(response.headers());
        let status = response.status();
        let text = response.text().await?;
        // 以前这里不看状态码：过期的 h5_url 或 CDN 错误页会被当成支付页去扫描，
        // 结果只报一个与真实原因无关的 `WeixinNotFound`。
        if !status.is_success() {
            return Err(PayError::api_error(status.as_u16(), &text, request_id));
        }
        text.lines()
            .find(|line| line.contains("weixin://"))
            .and_then(|line| line.split('"').find(|part| part.contains("weixin://")))
            .map(str::to_owned)
            .ok_or(PayError::WeixinNotFound)
    }

    /// 申请退款。
    ///
    /// 响应体与「查询退款」一致，因此返回 [`RefundsResponse`]。
    /// 退款是异步的：受理成功不等于退款成功，需用 [`WechatPay::query_refund`] 轮询
    /// `status` 直到离开 `PROCESSING`。
    #[maybe_async_attr]
    pub async fn refunds(&self, params: RefundsParams) -> Result<RefundsResponse, PayError> {
        let url = "/v3/refund/domestic/refunds";
        let body = params.to_json()?;
        self.request_json(HttpMethod::POST, url, &body, RequestKind::Refund)
            .await
    }

    /// 申请交易账单：返回账单文件的下载地址与 SHA1 摘要（文件本身再用
    /// [`WechatPay::download_bill`] 下载）。
    ///
    /// `GET /v3/bill/tradebill?bill_date=…&bill_type=…&tar_type=GZIP`
    ///
    /// 官方规则（对账口径）：
    /// * 微信在**每日 10 点后**生成**昨日**账单，`bill_date` 只支持三个月内的账单；
    /// * 账单里**只有支付成功的订单**，金额单位是**元**（保留两位小数），
    ///   下载到的文件需要按行解析；
    /// * 「账单文件不存在」返回 `NO_STATEMENT_EXIST`、「仍在生成」返回 `STATEMENT_CREATING`、
    ///   频率过高返回 429 `FREQUENCY_LIMITED` —— 都按 `PayError::ApiError` 的 `code` 区分。
    ///
    /// `bill_type` 建议用 [`BillType::Success`]（对账只需要成功支付的订单）。
    #[maybe_async_attr]
    pub async fn trade_bill(
        &self,
        bill_date: &str,
        bill_type: BillType,
        tar_type: Option<BillTarType>,
    ) -> Result<TradeBillResponse, PayError> {
        // bill_date 只允许 yyyy-MM-dd:它是**拼进签名串**的,形状不对会让签名与请求一起失败
        if !crate::util::is_bill_date(bill_date) {
            return Err(PayError::VerifyError(format!(
                "账单日期必须是 yyyy-MM-dd: {bill_date}"
            )));
        }
        let mut url = format!(
            "/v3/bill/tradebill?bill_date={bill_date}&bill_type={}",
            bill_type.as_str()
        );
        if let Some(tar_type) = tar_type {
            url.push_str("&tar_type=");
            url.push_str(tar_type.as_str());
        }
        self.get_pay(&url).await
    }

    /// 下载账单文件（第二步）：对 [`WechatPay::trade_bill`] 返回的 `download_url` 发**已签名** GET。
    ///
    /// ⚠ 与其余端点相反的两点，都是官方明文规定的：
    /// * 账单文件的应答**不带签名头** ⇒ 本方法**跳过应答验签**，也**不能**声称内容已验签；
    ///   完整性必须由调用方用 `hash_type` / `hash_value`（SHA1）自行比对；
    /// * 地址是**绝对的**且 5 分钟内有效 ⇒ 这里按官方给的地址**原样**请求（不套用网关前缀，
    ///   否则就把请求发到了另一个 host）；地址必须落在 `/v3/` 之下、默认必须 https
    ///   （只有调用方显式把客户端指向明文网关时才放行 http，见
    ///   `crate::util::bill_download_path_and_query`）。
    ///
    /// 返回**原始字节**：`tar_type = Some(BillTarType::Gzip)` 时是 gzip 流，由调用方解压。
    /// 非 2xx 会作为**未验签**的 `PayError::ApiError` 返回；下载失败直接重试即可
    /// （本方法内部已按只读请求的重试策略重试）。
    #[maybe_async_attr]
    pub async fn download_bill(&self, download_url: &str) -> Result<Vec<u8>, PayError> {
        let allow_plain_http = self.base_url().starts_with("http://");
        let path_and_query =
            crate::util::bill_download_path_and_query(download_url, allow_plain_http)?;
        let policy = self.policy_for(RequestKind::Read);
        let max_attempts = policy.max_attempts();
        let mut attempt: u32 = 1;

        loop {
            // 每次尝试重新签名(非 nonce + 新时间戳),签名串第二行用 path?query
            let headers = self.build_header(HttpMethod::GET, path_and_query, "")?;
            // ⚠ 只打印路径:下载地址的查询串里带 token,不能进日志。
            // 绑定名带下划线不是笔误:`debug!` 在未启用 `debug-print` 时展开为空操作,
            // 那时这个变量确实没被使用(见 `src/macros.rs`)。
            let _path_only = path_and_query.split('?').next().unwrap_or_default();
            debug!("download bill: {}", _path_only);
            let response = self
                .client
                .get(download_url)
                .headers(headers)
                .send()
                .await
                // `.without_url()`:地址里带 token,别让它经错误链进日志
                .map_err(|err| PayError::RequestError(err.without_url()))?;
            let status = response.status().as_u16();
            let request_id = request_id_of(response.headers());
            let body = response
                .bytes()
                .await
                .map_err(|err| PayError::RequestError(err.without_url()))?
                .to_vec();

            if (200..300).contains(&status) {
                return Ok(body);
            }
            // 非 2xx:官方用 JSON 错误信封(`{code,message}`)说明原因;这条应答没验签,如实标记
            let err =
                PayError::api_error_unverified(status, &String::from_utf8_lossy(&body), request_id);
            // 分类只在「可重试」时有意义(`classify` 对无需重试的错误返回 `None`)
            let delivery = classify(&err);
            let retry = delivery.is_some_and(|delivery| should_retry(delivery, RequestKind::Read));
            if !retry || attempt >= max_attempts {
                return Err(err);
            }
            let delay = policy.delay_for(attempt);
            debug!(
                "retry {}/{} after {:?} ({:?})",
                attempt, max_attempts, delay, delivery
            );
            attempt += 1;
            sleep_for(delay).await;
        }
    }

    /// 查询订单（按商户订单号）。
    ///
    /// `GET /v3/pay/transactions/out-trade-no/{out_trade_no}?mchid={mchid}`
    ///
    /// ⚠ `mchid` 是**唯一**的查询参数，且必须拼进 URL —— 微信签名串的第二行要求带上
    /// 查询串，漏掉会直接 401。订单不存在时微信返回 404 `ORDER_NOT_EXIST`，
    /// 会作为 `PayError::ApiError` 返回（`response.code`）。
    #[maybe_async_attr]
    pub async fn query_order(&self, out_trade_no: &str) -> Result<TransactionResponse, PayError> {
        // out_trade_no 与 mchid 的字符集被微信限制为数字/字母/`_`/`-`/`*`，无需 percent-encoding。
        let url = format!(
            "/v3/pay/transactions/out-trade-no/{out_trade_no}?mchid={}",
            self.mch_id()
        );
        self.request_json(HttpMethod::GET, &url, "", RequestKind::Read)
            .await
    }

    /// 关闭订单。
    ///
    /// `POST /v3/pay/transactions/out-trade-no/{out_trade_no}/close`，请求体**只有**
    /// `mchid` 一个字段。成功时微信返回 **204 No Content 且无响应体**，因此这里不解析
    /// body（用 `request_no_content` 而不是 `request_json`）。
    ///
    /// 注意：关单不是退款的替代 —— 已支付的订单只能退款。
    #[maybe_async_attr]
    pub async fn close_order(&self, out_trade_no: &str) -> Result<(), PayError> {
        let url = format!("/v3/pay/transactions/out-trade-no/{out_trade_no}/close");
        let body = serde_json::json!({ "mchid": self.mch_id() }).to_string();
        self.request_no_content(HttpMethod::POST, &url, &body, RequestKind::Write)
            .await
    }

    /// 查询退款（按商户退款单号）。
    ///
    /// `GET /v3/refund/domestic/refunds/{out_refund_no}` —— 该端点**没有查询参数**，
    /// `mchid` 由 Authorization 头隐含。响应体与「申请退款」一致，因此复用
    /// [`RefundsResponse`]。
    ///
    /// 退款是异步的：申请受理不等于退款成功，需要轮询本接口直到 `status` 离开
    /// `PROCESSING`（官方建议申请后每分钟查一次，5 分钟后降频）。
    /// 退款单不存在时微信返回 404 `RESOURCE_NOT_EXISTS`。
    #[maybe_async_attr]
    pub async fn query_refund(&self, out_refund_no: &str) -> Result<RefundsResponse, PayError> {
        let url = format!("/v3/refund/domestic/refunds/{out_refund_no}");
        self.request_json(HttpMethod::GET, &url, "", RequestKind::Read)
            .await
    }
}

#[cfg(test)]
mod tests {
    use crate::error::PayError;
    use dotenvy::dotenv;
    // 以下导入只被 `#[cfg(not(feature = "async"))]` 的测试使用，
    // 不加 cfg 会在 async 模式下产生 unused_imports 告警。
    #[cfg(not(feature = "async"))]
    use crate::model::{AppParams, H5Params, H5SceneInfo, JsapiParams, MicroParams};
    use crate::model::{NativeParams, RefundsParams};
    #[cfg(not(feature = "async"))]
    use crate::pay::PayNotifyTrait;
    use crate::pay::WechatPay;
    #[cfg(not(feature = "async"))]
    use crate::response::Certificate;
    #[cfg(not(feature = "async"))]
    use crate::util;
    #[cfg(not(feature = "async"))]
    use std::io::Write;
    use tracing::debug;

    #[test]
    #[cfg(not(feature = "async"))]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub fn test_jsapi_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .jsapi_pay(JsapiParams::new(
                "测试支付1分",
                "1243243",
                1.into(),
                "open_id".into(),
            ))
            .expect("jsapi_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    #[cfg(not(feature = "async"))]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub fn test_micro_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .micro_pay(MicroParams::new(
                "测试支付1分",
                "1243243",
                1.into(),
                "open_id".into(),
            ))
            .expect("micro_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    #[cfg(not(feature = "async"))]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub fn test_app_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .app_pay(AppParams::new("测试支付1分", "1243243", 1.into()))
            .expect("app_pay error");
        debug!("body: {:?}", body);
    }

    #[test]
    #[cfg(not(feature = "async"))]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub fn test_h5_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .h5_pay(H5Params::new(
                "测试支付1分",
                util::random_trade_no().as_str(),
                1.into(),
                H5SceneInfo::new("183.6.105.141", "ipa软件下载", "https://mydomain.com"),
            ))
            .expect("h5_pay error");
        let weixin_url = wechat_pay
            .get_weixin(body.h5_url.unwrap().as_str(), "https://mydomain.com")
            .unwrap();
        debug!("weixin_url: {}", weixin_url);
    }

    #[test]
    #[cfg(not(feature = "async"))]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub fn test_certificates() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let response = wechat_pay.certificates().expect("certificates error");
        let data = response.data.unwrap().first().unwrap().clone();
        let ciphertext = data.encrypt_certificate.ciphertext;
        let nonce = data.encrypt_certificate.nonce;
        let associated_data = data.encrypt_certificate.associated_data;
        let data = wechat_pay
            .decrypt_bytes(ciphertext, nonce, associated_data)
            .unwrap();
        let pub_key = util::x509_to_pem(data.as_slice()).unwrap();
        let mut pub_key_file = std::fs::File::create("pubkey.pem").unwrap();
        pub_key_file.write_all(pub_key.as_bytes()).unwrap();

        let (pub_key_valid, expire_timestamp) = util::x509_is_valid(data.as_slice()).unwrap();
        debug!(
            "pub key valid:{} expire_timestamp:{}",
            pub_key_valid, expire_timestamp
        ); //证书是否可用,过期时间
        debug!("pub key: {}", pub_key);
    }

    #[test]
    #[cfg(not(feature = "async"))]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub fn test_decode_certificates() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let response = wechat_pay.certificates().expect("certificates error");
        let data: Certificate = response.data.unwrap()[0].clone();
        let ciphertext = data.encrypt_certificate.ciphertext;
        let nonce = data.encrypt_certificate.nonce;
        let associated_data = data.encrypt_certificate.associated_data;
        let data = wechat_pay
            .decrypt_bytes(ciphertext, nonce, associated_data)
            .unwrap();
        debug!("data: {}", String::from_utf8_lossy(data.as_ref()));
    }

    #[test]
    #[cfg(not(feature = "async"))]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub fn test_blocking_refunds() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();

        let req = RefundsParams::new("123456", 1, 1, None, Some("123456"));

        match wechat_pay.refunds(req) {
            // 受理成功 ≠ 退款成功，需再用 query_refund 轮询 status 到终态
            Ok(body) => debug!(
                "refunds status: {} refund_id: {}",
                body.status, body.refund_id
            ),
            Err(PayError::ApiError {
                status, response, ..
            }) => {
                debug!("refunds failed: http {status}, {response}");
            }
            Err(e) => debug!("refunds error: {e}"),
        }
    }

    #[inline]
    fn init_log() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_line_number(true)
            .init();
    }

    /// 公开端点的 Future 必须是 `Send`。
    ///
    /// `tests/offline.rs::public_types_are_send_and_sync` 只断言**类型**是 `Send + Sync`，
    /// 抓不到「把 `RwLock` guard 跨 `.await` 持有」这类错误 —— 那种错误只在消费方
    /// （axum / actix 的处理函数）要求 Future 为 `Send` 时才暴露，而且是在用户那里暴露。
    /// 这里只**构造** Future、不轮询它，所以不需要 tokio runtime。
    #[test]
    #[cfg(feature = "async")]
    fn public_futures_are_send() {
        use crate::model::JsapiParams;

        fn assert_send<T: Send>(_: T) {}

        let wechat_pay = WechatPay::from_config(crate::pay::WechatPayConfig {
            appid: "appid".into(),
            mch_id: "mch".into(),
            private_key: "private key".into(),
            serial_no: "serial".into(),
            v3_key: "0123456789abcdef0123456789abcdef".into(),
            notify_url: "https://example.com/notify".into(),
            response_verify: crate::pay::ResponseVerify::Required,
        });

        assert_send(wechat_pay.query_order("ORDER"));
        assert_send(wechat_pay.close_order("ORDER"));
        assert_send(wechat_pay.jsapi_pay(JsapiParams::new("a", "O", 1.into(), "o".into())));
        assert_send(wechat_pay.fetch_platform_keys());
        assert_send(wechat_pay.refresh_platform_keys());
    }

    #[tokio::test]
    #[cfg(feature = "async")]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub async fn test_native_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .native_pay(NativeParams::new("测试支付1分", "1243243", 1.into()))
            .await
            .expect("pay fail");
        debug!("body: {:?}", body);
    }
    #[test]
    #[cfg(not(feature = "async"))]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub fn test_native_pay() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let body = wechat_pay
            .native_pay(NativeParams::new("测试支付1分", "1243243", 1.into()))
            .expect("pay fail");
        debug!("body: {:?}", body);
    }

    #[tokio::test]
    #[cfg(feature = "async")]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub async fn test_refunds() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();

        let req = RefundsParams::new("123456", 1, 1, None, Some("123456"));

        match wechat_pay.refunds(req).await {
            Ok(body) => debug!(
                "refunds status: {} refund_id: {}",
                body.status, body.refund_id
            ),
            Err(PayError::ApiError {
                status, response, ..
            }) => {
                debug!("refunds failed: http {status}, {response}");
            }
            Err(e) => debug!("refunds error: {e}"),
        }
    }

    #[test]
    #[cfg(not(feature = "async"))]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    pub fn test_refunds() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();

        let req = RefundsParams::new("123456", 1, 1, None, Some("123456"));

        match wechat_pay.refunds(req) {
            // 受理成功 ≠ 退款成功，需再用 query_refund 轮询 status 到终态
            Ok(body) => debug!(
                "refunds status: {} refund_id: {}",
                body.status, body.refund_id
            ),
            Err(PayError::ApiError {
                status, response, ..
            }) => {
                debug!("refunds failed: http {status}, {response}");
            }
            Err(e) => debug!("refunds error: {e}"),
        }
    }
}
