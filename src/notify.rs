//! 回调通知的校验：**新鲜度 → 按 serial 选键 → 验签**。
//!
//! 这三步缺一不可，少任何一步都有具体的资损路径：
//!
//! | 少做哪步 | 后果 |
//! | --- | --- |
//! | 不验签 | 任何人都能伪造回调让你发货 |
//! | 不查时间戳 | 抓到一次合法回调即可**无限重放** —— 发一次货，重复领取 |
//! | 写死单张证书 | 平台证书轮换期验签全部失败 → 回调被拒 → 订单不发货 |
//!
//! # 宿主时钟是硬依赖：NTP 是部署基线
//!
//! 这里的时间戳新鲜度窗口（[`MAX_TIMESTAMP_SKEW_SECS`](crate::notify::MAX_TIMESTAMP_SKEW_SECS)，
//! ±300s）与**出站应答**的验签共用：宿主时钟漂移会把**所有**回调与应答都判成超窗
//! （[`PayError::StaleNotify`](crate::error::PayError::StaleNotify)），表现成
//! 「全链路忽然都失败」。部署基线要求宿主时钟 **NTP 同步**；`StaleNotify` 的日志
//! **需要单独告警** —— 它更像「该对时了」的运维信号，不要直接当成攻击结论
//! （真正的重放只会零星出现，不会与时钟漂移造成的成片失败混淆）。
//!
//! SDK 刻意不做时钟健康检查：无法在本机可靠地测出自身偏移，为此加网络调用得不偿失。
//!
//! # 本模块**不**负责幂等
//!
//! 微信在收到成功应答前会重试投递（15s/15s/30s/3m/10m/20m/30m/…，最多 15 次），
//! 所以同一笔订单可能回调多次。幂等必须由业务侧落库完成：
//!
//! ```no_run
//! # use wechat_pay_rust_sdk::cert::PlatformKeys;
//! # use wechat_pay_rust_sdk::notify::NotifyHeaders;
//! # use wechat_pay_rust_sdk::pay::{PayNotifyTrait, WechatPay};
//! # fn handle(keys: &PlatformKeys, wechat_pay: &WechatPay, headers: NotifyHeaders, raw_body: &str) {
//! // 1. 校验来源与新鲜度（本模块）
//! keys.verify_notify(&headers, raw_body).expect("非法回调");
//!
//! // 2. 解密出业务数据（入口会先校验 resource.algorithm）
//! //    （实际字段来自 raw_body 的 resource 节点）
//! # use wechat_pay_rust_sdk::model::WechatPayNotifySource;
//! # let resource = WechatPayNotifySource {
//! #     algorithm: "AEAD_AES_256_GCM".into(),
//! #     ciphertext: String::new(),
//! #     associated_data: None,
//! #     original_type: "transaction".into(),
//! #     nonce: String::new(),
//! # };
//! let data = wechat_pay.decrypt_notify(&resource).expect("解密失败");
//!
//! // 3. 幂等：按 out_trade_no / transaction_id 落库去重，重复投递直接返回成功
//! //    —— 这一步必须由你的业务代码完成，SDK 无法代劳。
//! # }
//! ```
//!
//! 另外注意应答要求：**5 秒内**返回响应，成功时返回 **HTTP 200 或 204 且不带 body**；
//! 校验或处理失败才返回 4XX/5XX + `{"code":"FAIL","message":"…"}`。
//! 业务处理应当异步化，不要在回调线程里做重活。

use crate::cert::PlatformKeys;
use crate::error::PayError;

/// 官方建议允许的最大时间偏差：
/// 「如果时间戳与当前时间的偏差超过5分钟，您应拒绝处理当前的响应或回调通知」。
///
/// ⚠ 这是**宿主时钟**的硬依赖：回调与出站应答的验签共用这个窗口，主机时钟漂移会让两者
/// **全部**失败（[`PayError::StaleNotify`]）。部署基线要求 NTP 同步；拿到 `StaleNotify`
/// 先对时，并把它的日志**单独告警** —— 它不是「遭到攻击」的结论（见模块文档）。
pub const MAX_TIMESTAMP_SKEW_SECS: i64 = 5 * 60;

/// 校验 `Wechatpay-Timestamp` 与当前时间的偏差是否在 ±[`MAX_TIMESTAMP_SKEW_SECS`] 内。
///
/// 回调（[`PlatformKeys::verify_notify`]）与**出站应答**的验签（`WechatPay::verify_response`）
/// 共用这一份实现 —— 应答验签不允许另写一份：这段算术曾经出过事
/// （`Wechatpay-Timestamp` 是**未鉴权输入**，且在验签之前就被解析成整数，
/// `i64::MIN` 会让 `now - signed_at` 算术溢出，开启 overflow-checks 的构建里直接 panic）。
pub(crate) fn check_timestamp_skew(timestamp: &str, now_unix_secs: i64) -> Result<(), PayError> {
    let signed_at: i64 = timestamp.parse().map_err(|_| {
        PayError::StaleNotify(format!("Wechatpay-Timestamp 不是整数秒: {timestamp}"))
    })?;

    // 用无符号距离比较：`now - signed_at` 在 signed_at 取 i64::MIN 时会算术溢出。
    let skew = now_unix_secs.abs_diff(signed_at);
    if skew > MAX_TIMESTAMP_SKEW_SECS.unsigned_abs() {
        return Err(PayError::StaleNotify(format!(
            "时间戳偏差 {skew}s 超出 ±{MAX_TIMESTAMP_SKEW_SECS}s，判定为重放（timestamp={timestamp}, now={now_unix_secs}）"
        )));
    }
    Ok(())
}

/// 四个验签头的 HTTP 头名：`(小写名, 规范写法)`。
///
/// 顺序与 [`NotifyHeaders::new`] 的参数顺序一致（serial / timestamp / nonce / signature）。
const SIGNATURE_HEADERS: [(&str, &str); 4] = [
    ("wechatpay-serial", "Wechatpay-Serial"),
    ("wechatpay-timestamp", "Wechatpay-Timestamp"),
    ("wechatpay-nonce", "Wechatpay-Nonce"),
    ("wechatpay-signature", "Wechatpay-Signature"),
];

/// 从**应答**的请求头里取出四个签名头。
///
/// 返回 `Err(缺失的头名)` —— 缺头由调用方按状态码处置（2xx 拒绝、5xx 放行并标记）。
/// 非 UTF-8 的头值**视为缺失**：宁可拒绝，也不要拿替换字符去验签。
pub(crate) fn response_signature_headers(
    headers: &reqwest::header::HeaderMap,
) -> Result<NotifyHeaders, Vec<&'static str>> {
    let mut missing = Vec::new();
    let mut values: [String; 4] = Default::default();
    for (slot, (lowercase, display)) in values.iter_mut().zip(SIGNATURE_HEADERS) {
        match headers
            .get(lowercase)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
        {
            Some(value) => *slot = value,
            None => missing.push(display),
        }
    }
    if !missing.is_empty() {
        return Err(missing);
    }
    let [serial, timestamp, nonce, signature] = values;
    Ok(NotifyHeaders::new(serial, timestamp, nonce, signature))
}

/// 微信回调（及应答）的验签请求头。
#[derive(Debug, Clone)]
pub struct NotifyHeaders {
    /// `Wechatpay-Serial`：签名所用平台证书的序列号，或微信支付公钥 ID（`PUB_KEY_ID_…`）。
    pub serial: String,
    /// `Wechatpay-Timestamp`：签名时间戳（秒）。
    pub timestamp: String,
    /// `Wechatpay-Nonce`：签名随机串。
    pub nonce: String,
    /// `Wechatpay-Signature`：base64 签名。
    pub signature: String,
}

impl NotifyHeaders {
    /// 直接由四个请求头的值构造。
    ///
    /// 参数顺序：`serial` / `timestamp` / `nonce` / `signature`。
    /// 从 web 框架的 header 迭代器构造请用 [`NotifyHeaders::from_pairs`]。
    pub fn new(
        serial: impl Into<String>,
        timestamp: impl Into<String>,
        nonce: impl Into<String>,
        signature: impl Into<String>,
    ) -> Self {
        Self {
            serial: serial.into(),
            timestamp: timestamp.into(),
            nonce: nonce.into(),
            signature: signature.into(),
        }
    }

    /// 从 `(name, value)` 对中提取四个头。
    ///
    /// header 名比较时忽略大小写，并把 `_` 视作 `-`，因此 axum / actix 的
    /// header 迭代器都能直接传进来。四个头缺任意一个都会返回
    /// [`PayError::VerifyError`]。
    pub fn from_pairs<'a, I>(pairs: I) -> Result<Self, PayError>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut serial = None;
        let mut timestamp = None;
        let mut nonce = None;
        let mut signature = None;

        for (name, value) in pairs {
            let normalized = name.to_ascii_lowercase().replace('_', "-");
            match normalized.as_str() {
                "wechatpay-serial" => serial = Some(value.to_string()),
                "wechatpay-timestamp" => timestamp = Some(value.to_string()),
                "wechatpay-nonce" => nonce = Some(value.to_string()),
                "wechatpay-signature" => signature = Some(value.to_string()),
                _ => {}
            }
        }

        let missing = |name: &str| PayError::VerifyError(format!("回调缺少请求头 {name}"));
        Ok(Self {
            serial: serial.ok_or_else(|| missing("Wechatpay-Serial"))?,
            timestamp: timestamp.ok_or_else(|| missing("Wechatpay-Timestamp"))?,
            nonce: nonce.ok_or_else(|| missing("Wechatpay-Nonce"))?,
            signature: signature.ok_or_else(|| missing("Wechatpay-Signature"))?,
        })
    }
}

impl PlatformKeys {
    /// 一站式回调校验：**时间戳新鲜度 → 按 serial 选键 → 验签**。
    ///
    /// 通过后仍需自行完成幂等去重（见模块级文档）。
    /// 返回 [`PayError::StaleNotify`] 表示时间戳超窗（疑似重放）；
    /// 返回 [`PayError::UnknownPlatformSerial`] 表示该 serial 不在索引里 ——
    /// 应立即重新拉取平台证书列表后重试。
    pub fn verify_notify(&self, headers: &NotifyHeaders, body: &str) -> Result<(), PayError> {
        self.verify_notify_at(headers, body, crate::util::now_unix_secs())
    }

    /// 同 [`Self::verify_notify`]，但由调用方显式提供「当前时间」（unix 秒）。
    ///
    /// 供测试与需要注入时钟的场景使用。
    pub fn verify_notify_at(
        &self,
        headers: &NotifyHeaders,
        body: &str,
        now_unix_secs: i64,
    ) -> Result<(), PayError> {
        check_timestamp_skew(&headers.timestamp, now_unix_secs)?;

        self.verify(
            &headers.serial,
            &headers.timestamp,
            &headers.nonce,
            body,
            &headers.signature,
        )
    }
}
