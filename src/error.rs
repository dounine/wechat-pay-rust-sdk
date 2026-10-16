use crate::response::ErrorResponse;

/// 本 crate 的统一错误类型。
///
/// 用 [`PayError::kind`] 做三层归类（网络 / 业务 / 本地）来决定处置策略；
/// 微信侧的业务错误统一是 [`PayError::ApiError`]，原始错误码与字段级 `detail` 都在里面。
#[derive(Debug, thiserror::Error)]
pub enum PayError {
    /// 传输 / 连接层失败（含超时、TLS、DNS）。
    #[error("http error: {0}")]
    RequestError(#[from] reqwest::Error),
    /// 本地签名相关失败：商户私钥 PEM 无法解析、RSA 签名运算失败，或签名请求头无法构造
    /// （商户号 / 序列号含非法 HTTP 头字符）。
    #[error("sign error: {0}")]
    SignError(String),
    /// 响应体不是合法 JSON，或结构与目标类型不符。
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
    /// 回调 / 应答解密失败（AES-256-GCM）。
    #[error("Decrypt error: {0}")]
    DecryptError(String),
    /// Base64 解码失败。
    #[error("Base64 decode error: {0}")]
    DecodeError(#[from] base64::DecodeError),
    /// 验签失败。
    #[error("verify error: {0}")]
    VerifyError(String),
    /// 在 H5 页面里没找到 `weixin://` 拉起链接。
    #[error("weixin not found error")]
    WeixinNotFound,
    /// 平台上没有该 `Wechatpay-Serial` 对应的密钥。
    ///
    /// 通常意味着微信正在轮换平台证书（轮换期会同时下发新旧两张）——
    /// 应当**立即重新拉取**平台证书列表（`WechatPay::fetch_platform_keys`）后重试，
    /// 而不是拿别的密钥去试。
    #[error("unknown platform serial: {0}")]
    UnknownPlatformSerial(String),
    /// 回调/应答的时间戳超出允许窗口，判定为重放，已拒绝处理。
    #[error("stale notify rejected: {0}")]
    StaleNotify(String),
    /// 微信侧返回的业务错误：HTTP 非 2xx，或 2xx 但 body 是错误信封
    /// （重试用尽后降级的 202 也走这里）。
    ///
    /// ⚠ `status` 是微信返回的**原始**状态码，可能是 200 / 202 —— 不要假设它 ≥ 400。
    ///
    /// 保留微信原始的错误码、错误信息与 detail，便于定位到具体字段。
    /// 匹配方式：`PayError::ApiError { status, response, request_id }`（不需全部字段时用 `..`），
    /// 用 `response.code` 分支处理。
    ///
    /// `request_id` 取自应答头 `Request-ID`（微信侧的请求唯一标识，官方要求商户把它写进日志）——
    /// 退款未受理、查单异常这类需要微信侧协助排查的场景，提供该值即可快速定位到请求记录。
    ///
    /// ⚠ 日志提示：`Display` 会带上 `detail`，而微信的 `detail` 含出错字段的路径与取值
    /// （例如 `/payer/openid`），`message` 也可能回显提交内容。日志外发前需自行脱敏。
    #[error("wechat api error: http {status}, {response}{}", .request_id.as_deref().map(|id| format!(" (request-id: {id})")).unwrap_or_default())]
    ApiError {
        /// HTTP 状态码
        status: u16,
        /// 微信返回的错误体
        response: ErrorResponse,
        /// 应答头 `Request-ID`：微信侧的请求唯一标识（官方要求商户把它写进日志）。
        /// 应答缺该头、或被网关 / CDN 过滤时为 `None`。
        request_id: Option<String>,
    },
}

/// `PayError` 的三层归类。
///
/// 用来决定处置策略 —— 重试、告警、还是把原因直接展示给用户：
///
/// | 归类 | 含义 | 建议处置 |
/// | --- | --- | --- |
/// | [`Network`](ErrorKind::Network) | 传输 / 连接层失败 | 交给 [`crate::retry`] 判断。确定没送到的可安全重试；⚠ 结果未知的（读写超时）**写接口不重试**，应先查单确认（见 [`PayError::may_have_taken_effect`]） |
/// | [`Api`](ErrorKind::Api) | 微信侧业务拒绝（HTTP 非 2xx，或 2xx 的错误信封） | 交给 [`crate::retry`] 判定：429 / 500 / 502 / 503 与 `SYSTEM_ERROR` 会自动重试；其余按 `response.code` 分支，不要重试。`ORDER_NOT_EXIST` 这类是**正常业务结果**，不是故障 |
/// | [`Local`](ErrorKind::Local) | 本地错误：签名、解密、Base64、JSON 解析、验签失败、回调超窗 | 不要重试，通常意味着配置或数据有问题，应当告警 |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// 传输 / 连接层失败（`reqwest::Error`，含超时）。
    Network,
    /// 微信侧的业务拒绝：HTTP 非 2xx，或 2xx 但 body 是错误信封。
    Api,
    /// 本地错误：签名、解密、Base64、JSON 解析、验签失败、回调超窗等。
    Local,
}

impl PayError {
    /// 三层归类，见 [`ErrorKind`]。
    ///
    /// 这里是**穷尽匹配**：`PayError` 新增变体时编译器会强制你在这里做出归类决定。
    pub fn kind(&self) -> ErrorKind {
        match self {
            PayError::RequestError(_) => ErrorKind::Network,
            PayError::ApiError { .. } => ErrorKind::Api,
            PayError::SignError(_)
            | PayError::JsonError(_)
            | PayError::DecryptError(_)
            | PayError::DecodeError(_)
            | PayError::VerifyError(_)
            | PayError::WeixinNotFound
            | PayError::UnknownPlatformSerial(_)
            | PayError::StaleNotify(_) => ErrorKind::Local,
        }
    }

    /// 这次失败是否**可能已经在微信侧生效**。
    ///
    /// `true` 表示请求确实送到了微信、但没拿到确定结果 —— 读写超时、响应体没读完、
    /// HTTP 202（已受理但尚未处理完），或响应体解析失败。此时**不要**当成「操作失败」
    /// 处理：支付、退款这类接口应当先用
    /// [`WechatPay::query_order`](crate::pay::WechatPay::query_order) /
    /// [`WechatPay::query_refund`](crate::pay::WechatPay::query_refund) 确认最终状态，
    /// 再决定下一步。**这正是 SDK 不对写接口超时做自动重试的原因**
    /// （见 [`crate::retry`]）。
    ///
    /// `false` 表示可以确定微信没有受理这次请求：连接根本没建立起来（连接被拒 /
    /// 连接超时），微信明确说未受理（429 / 500 / 502 / 503、`SYSTEM_ERROR`），
    /// 或者是签名 / 解密 / 请求体编码这类**本地**错误。
    ///
    /// ⚠ 三类容易被想当然的边界，都刻意偏保守：
    ///
    /// - **HTTP 202** 是「已接受请求，但尚未处理」，官方要求「请使用原参数重复请求
    ///   一遍」—— 它返回 `true`。请求已经被接收，可能随后生效。
    /// - **响应体解析失败**也返回 `true`：响应都回来了，说明请求早已送达。
    /// - **应答验签失败**（[`PayError::VerifyError`] / [`PayError::UnknownPlatformSerial`] /
    ///   [`PayError::StaleNotify`]）同样返回 `true`：应答是**完整收到之后**才验签的，
    ///   而且官方会在极少数应答里故意下发错误签名来探测商户的验签实现 ——
    ///   这个失败与被处理与否无关。若按 `false` 处理，写接口可能被换个单号重开。
    ///
    /// 判错的方向性代价并不对称 —— 多判成 `true` 只是让调用方多查一次单（`ORDER_NOT_EXIST`
    /// 会如实告诉他没这回事），漏判成 `false` 则可能诱导他换个单号重新下单。
    ///
    /// ```
    /// # use wechat_pay_rust_sdk::error::PayError;
    /// # fn handle(err: PayError) {
    /// if err.may_have_taken_effect() {
    ///     // 去查单确认，而不是直接重试下单
    /// }
    /// # }
    /// ```
    pub fn may_have_taken_effect(&self) -> bool {
        matches!(
            crate::retry::classify(self),
            Some(
                crate::retry::Delivery::Unknown
                    | crate::retry::Delivery::Processed
                    | crate::retry::Delivery::Accepted
            )
        )
    }
}

/// 原始响应体保留进错误消息时的最大字符数。
/// 防止网关 / WAF 返回的整页 HTML 被复制进错误消息并刷爆日志。
const MAX_RAW_BODY_CHARS: usize = 4096;

impl PayError {
    /// 由非 2xx 响应体构造业务错误。
    ///
    /// 仅当响应体**确实是微信的错误结构**（至少含 `code` / `message` / `detail` 之一）
    /// 时才接受结构化解析；否则把原始文本截断后放进 `message`。
    ///
    /// 不能只判断「能否解析成 JSON 对象」：`ErrorResponse` 的字段全是 `Option` 且未加
    /// `deny_unknown_fields`，任何 JSON 对象都能解析成功。若不加这层判断，WAF / 反向
    /// 代理返回的 `{"status":403,"msg":"..."}` 会变成一个三个字段全为 `None` 的
    /// `ErrorResponse`，唯一的排查线索（原始文本）就被静默丢弃了。
    pub(crate) fn api_error(status: u16, body: &str, request_id: Option<String>) -> Self {
        PayError::ApiError {
            status,
            response: parse_error_response(body),
            request_id,
        }
    }

    /// 同 [`PayError::api_error`]，但给 `message` 打上 `[未验签]` 标记。
    ///
    /// 用于「5xx 且没有签名头」这一类：链路（CDN / 反向代理 / WAF）过滤了
    /// `Wechatpay-*` 头，此时既不能验签、也不该把响应丢掉 —— 5xx 的错误体里没有
    /// 可被对手用来误判业务的语义，而把它变成 `VerifyError` 会把「自动重试」变成
    /// 「不重试」。标记保留 `code` / `detail`，因此 [`crate::retry::classify`] 的
    /// `SYSTEM_ERROR` 判定不受影响。
    pub(crate) fn api_error_unverified(
        status: u16,
        body: &str,
        request_id: Option<String>,
    ) -> Self {
        let mut response = parse_error_response(body);
        response.message = Some(match response.message.take() {
            Some(message) => format!("[未验签] {message}"),
            None => "[未验签]".to_string(),
        });
        PayError::ApiError {
            status,
            response,
            request_id,
        }
    }
}

/// 把响应体解析成微信的错误结构；不是微信形状时把原文截断后放进 `message`。
fn parse_error_response(body: &str) -> ErrorResponse {
    let parsed = serde_json::from_str::<ErrorResponse>(body)
        .ok()
        .filter(|r| r.code.is_some() || r.message.is_some() || r.detail.is_some());

    parsed.unwrap_or_else(|| ErrorResponse {
        code: None,
        message: Some(truncate_raw_body(body)),
        detail: None,
    })
}

/// 截断过长的响应体并明确标注截断，空响应体给出显式标记。
pub(crate) fn truncate_raw_body(body: &str) -> String {
    if body.is_empty() {
        return "<empty body>".to_string();
    }
    if body.chars().count() <= MAX_RAW_BODY_CHARS {
        return body.to_string();
    }
    let head: String = body.chars().take(MAX_RAW_BODY_CHARS).collect();
    format!("{head}… (truncated, {} bytes total)", body.len())
}
