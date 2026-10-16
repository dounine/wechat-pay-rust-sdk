//! 自动重试：策略、失败分类与「这个错误能不能重试」的判定。
//!
//! # 为什么重试要分成三六九等
//!
//! 重试安不安全，**只取决于一件事**：上一次尝试到底有没有被微信处理过。
//! 同样是「失败」，下面五种情况的后果完全不同：
//!
//! | 分类 | 典型场景 | 微信侧状态 | 能否重放 |
//! | --- | --- | --- | --- |
//! | `Delivery::NotSent` | 连接被拒、连接超时 | **确定没收到** | 任何接口都可以 |
//! | `Delivery::Rejected` | 429 / 500 / 502 / 503 | 官方标注「未受理 / 无法处理」 | 任何接口都可以 |
//! | `Delivery::Accepted` | HTTP 202 | **已收到**，只是尚未处理完 | 任何接口都可以 |
//! | `Delivery::Unknown` | 连上了但读不到响应（读写超时） | **可能已经处理** | 只有只读接口敢 |
//! | `Delivery::Processed` | 响应体读了一半断连 | **已经处理过** | 一律不重放 |
//!
//! 前三类的判据来自微信官方文档（HTTP 状态码页把 429 写成「请求未受理」、
//! 502/503 写成「请求无法处理」，202 写成「已接受请求，但尚未处理，请使用原参数重复
//! 请求一遍」，各接口的 500 都写「请用相同参数重新调用」）；
//! 这些界线则来自 reqwest 的错误标志位实测（`is_connect` / `is_decode` /
//! `is_timeout` 能把上表干净地分开）。
//!
//! # 为什么「结果未知」时写接口不重试
//!
//! 微信下单以 `out_trade_no`、退款以 `out_refund_no` 作为身份键，SDK 内部重试又是
//! 字节完全一致的 —— 所以重放**不会**产生第二笔。但官方对超时的口径是「先用查单接口
//! 确认状态」，而不是「直接再发一次」。因此默认策略是：
//!
//! - 只读接口：超时照常重试（重放纯读没有副作用）
//! - 写接口（下单 / 关单）与退款：超时**不重试**，由调用方用
//!   [`PayError::may_have_taken_effect`](crate::error::PayError::may_have_taken_effect)
//!   判断后调 `query_order` / `query_refund` 确认
//!
//! # 退避为什么分两档
//!
//! 退款失败后官方给的节奏是「间隔 1 分钟」再重试，且接口在**失败时**限流只有 6QPS ——
//! 秒级退避打过去基本是白打，还会加重限流。所以退款单独走分钟级策略
//! （[`RetryPolicy::for_refund`](crate::retry::RetryPolicy::for_refund)），
//! 其余接口走毫秒级。
//! ⚠ 分钟级退避意味着退款重试会**阻塞到分钟级**，同步调用链里要留意，
//! 必要时用 [`RetryPolicy::disabled`](crate::retry::RetryPolicy::disabled)
//! 关掉并由业务侧异步重试。

use crate::error::PayError;
use std::time::Duration;

/// 默认最大尝试次数（**含首次**）：3 次。
pub const DEFAULT_MAX_ATTEMPTS: u32 = 3;
/// 默认首次退避间隔：200ms。
pub const DEFAULT_BASE_DELAY: Duration = Duration::from_millis(200);
/// 默认单次退避上限：2s。
pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(2);

/// 退款专用最大尝试次数（含首次）：2 次。
///
/// 刻意比通用策略少 —— 官方给的退款重试节奏是分钟级，多试几次会把同步调用链
/// 拖到好几分钟。
pub const DEFAULT_REFUND_MAX_ATTEMPTS: u32 = 2;
/// 退款专用首次退避间隔：60s。对应官方「间隔 1 分钟」的建议。
pub const DEFAULT_REFUND_BASE_DELAY: Duration = Duration::from_secs(60);
/// 退款专用单次退避上限：120s。
pub const DEFAULT_REFUND_MAX_DELAY: Duration = Duration::from_secs(120);

/// 重试策略：最多试几次、每次之间等多久。
///
/// 用 [`WechatPay::with_retry`](crate::pay::WechatPay::with_retry) 覆盖，
/// 配合 struct update 语法只改想改的那项。
///
/// ```
/// use wechat_pay_rust_sdk::retry::RetryPolicy;
/// use std::time::Duration;
///
/// // 关掉重试
/// let off = RetryPolicy::disabled();
/// // 只改次数，其余沿用默认
/// let more = RetryPolicy { max_attempts: 5, ..RetryPolicy::default() };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// 最多尝试几次，**含首次**。`1` 表示不重试。
    ///
    /// `0` 会被当作 `1`（即不重试）处理，不会出现「一次都不发」。
    pub max_attempts: u32,
    /// 退避基准：首次重试等这么久，之后按 2 的幂增长，直到 `max_delay`。
    ///
    /// ⚠ 开了 [`jitter`](Self::jitter) 时它是**上限**而不是确定值 —— 实际等待是
    /// `0..=计算结果` 上的随机数。需要「至少等这么久」的语义（例如官方对退款要求的
    /// 最小间隔），请把 `jitter` 设为 `false`。
    pub base_delay: Duration,
    /// 退避增长的上限。
    ///
    /// ⚠ 抖动是在截断**之后**施加的，所以开启抖动时实际等待只会比它更短。
    pub max_delay: Duration,
    /// 是否给退避加**全抖动**：实际等待取 `0..=计算值` 上的随机数。
    ///
    /// 微信侧大面积故障时，所有客户端会在同一刻失败、又在同一刻重发；
    /// 抖动把这批重发摊开，避免把刚恢复的服务再打垮。
    ///
    /// ⚠ 它是**向下**抖动的，因此开启后 `base_delay` 与 `max_delay` 都退化为上限。
    /// 若某个档位的等待时点是外部的硬要求（退款就是），关掉它。
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            base_delay: DEFAULT_BASE_DELAY,
            max_delay: DEFAULT_MAX_DELAY,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// 完全关闭重试：只发一次，失败即返回。
    pub fn disabled() -> Self {
        Self {
            max_attempts: 1,
            ..Self::default()
        }
    }

    /// 退款专用策略：分钟级退避。
    ///
    /// 依据是官方对退款重试的节奏要求（「间隔 1 分钟」）以及接口在失败时报错
    /// 限流仅 6QPS 的约束。
    ///
    /// ⚠ 这里**刻意关掉抖动**：`base_delay` 是官方要求的最小间隔，而全抖动会把实际
    /// 等待摊到 `0..=base_delay`（可以接近 0），等于把「间隔 1 分钟」变成「可能立刻
    /// 重发」—— 正是本策略要避免的事。代价是重试时点不再分散；退款并发量小，
    /// 这个代价可以接受。
    pub fn for_refund() -> Self {
        Self {
            max_attempts: DEFAULT_REFUND_MAX_ATTEMPTS,
            base_delay: DEFAULT_REFUND_BASE_DELAY,
            max_delay: DEFAULT_REFUND_MAX_DELAY,
            jitter: false,
        }
    }

    /// 实际生效的最大尝试次数。把 `0` 兜成 `1`，保证至少发一次。
    pub(crate) fn max_attempts(&self) -> u32 {
        self.max_attempts.max(1)
    }

    /// 第 `retry_index` 次重试（从 `1` 开始）之前应当等待多久。
    pub(crate) fn delay_for(&self, retry_index: u32) -> Duration {
        // 2 的幂增长，指数上限 16 次方避免移位溢出；再叠加 saturating_mul 兜底。
        let exponent = retry_index.saturating_sub(1).min(16);
        let grown = self.base_delay.saturating_mul(1u32 << exponent);
        let capped = grown.min(self.max_delay);
        if self.jitter {
            random_upto(capped)
        } else {
            capped
        }
    }
}

/// 在 `0..=cap` 之间取一个随机时长（全抖动）。
///
/// 用 `uuid` 的 v4 随机数当熵源，避免为了抖动再引入一个随机数依赖。
fn random_upto(cap: Duration) -> Duration {
    if cap.is_zero() {
        return cap;
    }
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let raw = u64::from_le_bytes(bytes[..8].try_into().expect("取 8 字节"));
    let nanos = cap.as_nanos().min(u128::from(u64::MAX));
    Duration::from_nanos((u128::from(raw) * nanos / u128::from(u64::MAX)) as u64)
}

/// 一次失败的请求在微信侧「可能处于什么状态」。
///
/// 这是重试判定的唯一依据，也是调用方判断「该不该去查单」的依据 ——
/// 详见 [`PayError::may_have_taken_effect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// 请求确定没送到微信：连接被拒，或连 TCP 都没建立起来就超时。
    NotSent,
    /// 微信明确表示没有受理这次请求：429 / 500 / 502 / 503，以及错误码为
    /// `SYSTEM_ERROR` 的 2xx 错误信封（与 HTTP 500 同源）。
    Rejected,
    /// 微信**已经收到**这次请求，只是尚未处理完：HTTP 202。
    ///
    /// ⚠ 与 [`Delivery::Rejected`] 的区别很关键：请求已被受理，**可能随后生效**。
    /// 官方对 202 的要求是「请使用原参数重复请求一遍」，所以照样重试；
    /// 但绝不能当成「没发生」—— 它的「可能已生效」判定是 `true`。
    Accepted,
    /// 请求送到了，但结果未知：连上了、读不到响应（读写超时），或官方未定义处置的
    /// 5xx（504 / 501 / 505，以及 CDN 的 520 一类）。
    Unknown,
    /// 响应都开始返回了，说明微信**已经处理过**这次请求：读响应体失败属于这类。
    Processed,
}

/// 把 `PayError` 归类成 [`Delivery`]；返回 `None` 表示这个错误不具备重试价值。
///
/// 判定顺序有讲究：**先看 `is_connect()`** —— 它同时覆盖「连接被拒」与「连接超时」
/// 两种情况，且都意味着请求没送出去。若先判 `is_timeout()`，连接超时会被误归成
/// 「结果未知」，白白丢掉一个绝对安全的重试机会。
///
/// 另一个容易写错的地方：读响应体失败报的是 **`is_decode()`**，不是 `is_body()`
/// （实测 `is_body()` 为 false）。按直觉写 `is_body()` 会漏掉这一类，
/// 而它恰恰是「服务端已经处理过、绝不能重放」的那种。
pub(crate) fn classify(err: &PayError) -> Option<Delivery> {
    match err {
        PayError::RequestError(e) => {
            if e.is_connect() {
                Some(Delivery::NotSent)
            } else if e.is_decode() || e.is_body() {
                Some(Delivery::Processed)
            } else {
                // 含读写超时：请求已经送出，但结果未知。保守归类 ——
                // 写接口不会因此被重放。
                Some(Delivery::Unknown)
            }
        }
        PayError::ApiError {
            status, response, ..
        } => {
            // 错误码优先于状态码：微信会以 HTTP 200 返回错误信封，那时状态码没有意义。
            // SYSTEM_ERROR 与 HTTP 500 同源，官方要求「请用相同参数重新调用」。
            if response.code.as_deref() == Some("SYSTEM_ERROR") {
                return Some(Delivery::Rejected);
            }
            match *status {
                // 官方 HTTP 状态码页：429「请求未受理」、502/503「请求无法处理」；
                // 各接口的 500 都写「请用相同参数重新调用」。
                429 | 500 | 502 | 503 => Some(Delivery::Rejected),
                // 202 是「已受理、尚未处理」：能重试，但**不等于没发生**。
                202 => Some(Delivery::Accepted),
                // 官方状态码表里没有 504，也没有 501 / 505 / CDN 的 520 一类。
                // 不替它下结论 —— 按「结果未知」处理最保守：只读可重试、写不重试，
                // 且「可能已生效」为 true。
                status if (500..600).contains(&status) => Some(Delivery::Unknown),
                // 4xx 是参数 / 权限 / 签名问题，重试不会有不同结果。
                _ => None,
            }
        }
        // 响应体解析失败：只可能发生在**已经拿到响应**之后（`pay()` 里构造请求体时
        // 的本地序列化失败属于 crate 自身 bug，实际不可达）。既然响应都回来了，请求
        // 必然已经送到微信 —— 按「可能已生效」处理。
        // 这个方向判错只会让调用方多查一次单；反方向判错可能导致重复下单。
        PayError::JsonError(_) => Some(Delivery::Processed),
        // 签名、解密、Base64 等本地错误：请求根本没发出去，重试结果一样。
        PayError::SignError(_)
        | PayError::DecryptError(_)
        | PayError::DecodeError(_)
        | PayError::WeixinNotFound => None,
        // 验签类错误（回调验签、**出站应答**验签、选键、超窗）：同样不重试，但**已经收到
        // 应答**了 —— 与上面的 `JsonError` 同类，按「可能已生效」处理。
        //
        // ⚠ 这里刻意**不**落进 `None`：`None` 同时意味着「不重试」与
        // `may_have_taken_effect() == false`，后者被文档描述为「可以确定微信没有受理
        // 这次请求」。出站应答验签失败时这个结论是**错的**：
        //
        // * 应答都完整回来了，说明请求早已送达微信（响应体读一半断连都算「已处理」）；
        // * 官方明确会在极少数应答里下发错误签名（`WECHATPAY/SIGNTEST/`）来探测商户的
        //   验签实现 —— 也就是说「验签失败」在生产中是预期事件，且与被处理与否无关；
        // * 最常见的真实原因是证书轮换：那一刻的应答是真的，订单**已经建了**。
        //
        // 对写接口（下单 / 关单 / 退款）判 `false` 会诱导调用方换个单号重开 —— 正是
        // [`PayError::may_have_taken_effect`] 那套取舍要避免的方向。
        PayError::VerifyError(_)
        | PayError::UnknownPlatformSerial(_)
        | PayError::StaleNotify(_) => Some(Delivery::Processed),
    }
}

/// 请求的重放语义。
///
/// 枚举**没有默认值**：新增接口必须显式标明语义，漏写就是编译错误 ——
/// 这比「有个保守默认值」更硬（`should_retry` 里没有 `_` 分支可以兜底）。
/// 代价是每加一个接口都得先想清楚它能不能重放，但这正是必须想清楚的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestKind {
    /// 只读查询：重放不改变任何状态，所以**结果未知也能重试**。
    Read,
    /// 写操作（下单、关单）：超时后结果未知，**不重试** —— 由调用方查单确认。
    Write,
    /// 退款：重放语义同 [`RequestKind::Write`]，但退避走分钟级策略。
    Refund,
}

/// 这次失败该不该重试。
///
/// | 失败分类 | 只读 | 写 / 退款 |
/// | --- | --- | --- |
/// | `NotSent`（确定没送到） | 重试 | 重试 |
/// | `Rejected`（微信说没受理） | 重试 | 重试 |
/// | `Accepted`（已受理未处理完） | 重试 | 重试 |
/// | `Unknown`（结果未知） | 重试 | **不重试** |
/// | `Processed`（已处理） | 不重试 | 不重试 |
pub(crate) fn should_retry(delivery: Delivery, kind: RequestKind) -> bool {
    match delivery {
        Delivery::NotSent | Delivery::Rejected | Delivery::Accepted => true,
        Delivery::Unknown => matches!(kind, RequestKind::Read),
        Delivery::Processed => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_max_attempts_still_sends_once() {
        let policy = RetryPolicy {
            max_attempts: 0,
            ..RetryPolicy::default()
        };
        assert_eq!(policy.max_attempts(), 1, "0 必须兜成 1，不能一次都不发");
    }

    #[test]
    fn delay_grows_exponentially_and_respects_cap() {
        let policy = RetryPolicy {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(500),
            jitter: false,
            ..RetryPolicy::default()
        };
        assert_eq!(policy.delay_for(1), Duration::from_millis(100));
        assert_eq!(policy.delay_for(2), Duration::from_millis(200));
        assert_eq!(policy.delay_for(3), Duration::from_millis(400));
        assert_eq!(
            policy.delay_for(4),
            Duration::from_millis(500),
            "应被上限截住"
        );
        assert_eq!(policy.delay_for(99), Duration::from_millis(500), "不应溢出");
    }

    #[test]
    fn jitter_stays_within_the_cap() {
        let policy = RetryPolicy {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(100),
            jitter: true,
            ..RetryPolicy::default()
        };
        let mut seen_nonzero = false;
        let mut seen_below_cap = false;
        for _ in 0..200 {
            let d = policy.delay_for(1);
            assert!(d <= Duration::from_millis(100), "抖动不得越过上限: {d:?}");
            seen_nonzero |= !d.is_zero();
            seen_below_cap |= d < Duration::from_millis(100);
        }
        assert!(seen_nonzero, "抖动应当真的随机，而不是恒为 0");
        // 「抖动被关掉、每次都返回计算值」也是一种坏实现，它会让上界断言照常通过。
        assert!(seen_below_cap, "抖动退化成了恒定上限值");
    }

    #[test]
    fn refund_policy_is_minute_scaled() {
        let policy = RetryPolicy::for_refund();
        assert!(
            policy.base_delay >= Duration::from_secs(30),
            "退款退避必须是分钟级（官方要求「间隔 1 分钟」）"
        );
        assert!(
            policy.max_attempts() <= 2,
            "退款不应重试太多次，否则阻塞数分钟"
        );
        assert!(
            !policy.jitter,
            "退款必须关掉抖动：全抖动会把实际等待摊到 0..=base_delay，\
             等于把「间隔 1 分钟」变成「可能立刻重发」"
        );
        assert_eq!(
            policy.delay_for(1),
            DEFAULT_REFUND_BASE_DELAY,
            "关掉抖动后，首次（也是唯一一次）重试必须等满 60s"
        );
        assert_eq!(RetryPolicy::disabled().max_attempts(), 1);
    }

    #[test]
    fn write_is_never_retried_when_the_outcome_is_unknown() {
        for kind in [RequestKind::Write, RequestKind::Refund] {
            assert!(
                !should_retry(Delivery::Unknown, kind),
                "结果未知时写接口不得重试：{kind:?}"
            );
            assert!(
                !should_retry(Delivery::Processed, kind),
                "已处理过的请求不得重放：{kind:?}"
            );
        }
    }

    #[test]
    fn always_retryable_classes_cover_every_kind() {
        for kind in [RequestKind::Read, RequestKind::Write, RequestKind::Refund] {
            for delivery in [Delivery::NotSent, Delivery::Rejected, Delivery::Accepted] {
                assert!(should_retry(delivery, kind), "{delivery:?} / {kind:?}");
            }
        }
        // 只读接口连「结果未知」都能重试。
        assert!(should_retry(Delivery::Unknown, RequestKind::Read));
    }

    #[test]
    fn may_have_taken_effect_is_false_when_wechat_never_took_the_request() {
        let api = |status: u16| PayError::api_error(status, "{}", None);
        for err in [
            // 官方明确说「请求未受理 / 请求无法处理」
            api(429),
            api(503),
            // 参数 / 权限 / 资源类错误：重试不会有不同结果
            api(400),
            api(404),
            // 本地错误：请求根本没发出去
            PayError::WeixinNotFound,
            PayError::SignError("bad key".into()),
        ] {
            assert!(
                !err.may_have_taken_effect(),
                "{err} 表示微信没有受理这次请求，不应被当成「结果未知」"
            );
        }
        // 「结果未知」的两类由集成测试用真实 HTTP 场景覆盖：
        //   tests/offline.rs::write_timeout_is_not_retried_but_read_timeout_is
        //   tests/offline.rs::response_body_that_dies_midway_is_never_retried
    }

    #[test]
    fn verification_failures_do_not_retry_but_may_have_taken_effect() {
        // 应答**已经完整收到**才谈得上验签失败 —— 请求必然已送达微信，且官方会故意
        // 下发带错误签名的探测应答。这里必须与「响应体解析失败」同判为 `true`：
        // 判 `false`（= 可以确定微信没有受理）会诱导写接口换个单号重开。
        for err in [
            PayError::VerifyError("bad signature".into()),
            PayError::UnknownPlatformSerial("SERIAL".into()),
            PayError::StaleNotify("stale".into()),
        ] {
            assert!(
                err.may_have_taken_effect(),
                "{err} 是「已收到应答但没通过验签」，不能当成「确定没发生」"
            );
            assert_eq!(
                classify(&err),
                Some(Delivery::Processed),
                "{err} 必须归为「已处理」：不重试，但可能已生效"
            );
            assert_eq!(err.kind(), crate::error::ErrorKind::Local);
        }
    }

    #[test]
    fn api_status_classification_matches_official_guidance() {
        let api = |status: u16| PayError::api_error(status, "{}", None);
        for retryable in [429, 500, 502, 503] {
            assert_eq!(
                classify(&api(retryable)),
                Some(Delivery::Rejected),
                "{retryable} 官方明确说未受理/无法处理，应当可重试"
            );
        }
        assert_eq!(
            classify(&api(202)),
            Some(Delivery::Accepted),
            "202 是「已接受、尚未处理」，不是「没受理」—— 能重试但**不等于没发生**"
        );
        for undefined in [501, 504, 505, 520] {
            assert_eq!(
                classify(&api(undefined)),
                Some(Delivery::Unknown),
                "官方状态码表没有 {undefined}，只能按「结果未知」处理"
            );
        }
        // 2xx + SYSTEM_ERROR 信封与 HTTP 500 同源，官方要求「请用相同参数重新调用」。
        let envelope =
            PayError::api_error(200, r#"{"code":"SYSTEM_ERROR","message":"系统异常"}"#, None);
        assert_eq!(
            classify(&envelope),
            Some(Delivery::Rejected),
            "错误码应当优先于状态码：HTTP 200 的 SYSTEM_ERROR 同样要重试"
        );
        for permanent in [400, 401, 403, 404, 405] {
            assert_eq!(classify(&api(permanent)), None, "{permanent} 重试没有意义");
        }
    }

    #[test]
    fn accepted_and_unparsable_responses_may_have_taken_effect() {
        // 202：微信已经受理，只是还没处理完 —— 可能随后生效，不能当成「没发生」。
        assert!(
            PayError::api_error(202, "", None).may_have_taken_effect(),
            "202 的请求已被微信接收，调用方必须去查单而不是换个单号重开"
        );
        // 响应体解析失败：响应都回来了，请求必然已送达。这里刻意偏保守 ——
        // 判成 false 会让调用方以为「没发生」，进而可能重复下单。
        let parse_error = serde_json::from_str::<i32>("not json").expect_err("应当解析失败");
        assert!(PayError::JsonError(parse_error).may_have_taken_effect());
    }
}
