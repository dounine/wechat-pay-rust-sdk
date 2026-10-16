use crate::cert::{DEFAULT_KEY_REFRESH_WAIT, PlatformKeys};
use crate::error::PayError;
use crate::macros::debug;
use crate::model::{WechatPayDecodeData, WechatPayNotifySource, WechatPayRefundDecodeData};
use crate::request::HttpMethod;
use crate::response::SignData;
use crate::retry::{RequestKind, RetryPolicy};
use crate::{sign, util};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, USER_AGENT};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use uuid::Uuid;

#[cfg(feature = "async")]
use reqwest::Client;
#[cfg(not(feature = "async"))]
use reqwest::blocking::Client;

/// 默认连接超时：5 秒。
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 默认整请求超时：10 秒。
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// 默认连接池空闲连接保留时长：90 秒。
pub const DEFAULT_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// HTTP 客户端超时配置。
///
/// 用 [`WechatPay::with_timeouts`] 覆盖，配合 struct update 语法只改想改的那项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpTimeouts {
    /// TCP / TLS 建连超时。
    pub connect: Duration,
    /// 整个请求的超时 —— 从开始建连到响应体读完。
    pub request: Duration,
    /// 连接池里空闲连接的保留时长，决定连接能复用多久。
    pub pool_idle: Duration,
}

impl Default for HttpTimeouts {
    fn default() -> Self {
        Self {
            connect: DEFAULT_CONNECT_TIMEOUT,
            request: DEFAULT_REQUEST_TIMEOUT,
            pool_idle: DEFAULT_POOL_IDLE_TIMEOUT,
        }
    }
}

/// 构建带超时的 HTTP 客户端。
///
/// ⚠ 超时**不代表操作没有发生**：请求可能已在微信侧受理，只是响应没回来。
/// 这类「结果未知」的失败（[`PayError::may_have_taken_effect`] 为 `true`）不会被
/// 自动重试 —— 支付类接口应当先用 [`WechatPay::query_order`] 确认最终状态。
///
/// 需要说明的是：微信以 `out_trade_no` 作为订单的身份键，**同一单号 + 完全相同的参数**
/// 重复下单不会产生第二笔订单（只有改参数或换接口才会报 `OUT_TRADE_NO_USED`）。
/// 所以这里要求先查单，是遵循官方「结果未知时以查单为准」的口径，
/// 而不是因为重放本身会重复下单 —— 详见 [`crate::retry`] 的模块文档。
fn build_client(timeouts: HttpTimeouts) -> Client {
    Client::builder()
        .connect_timeout(timeouts.connect)
        .timeout(timeouts.request)
        .pool_idle_timeout(timeouts.pool_idle)
        .build()
        .expect("构建 HTTP 客户端失败（TLS 后端初始化失败）")
}

/// 构造 [`WechatPay`] 所需的全部凭据。
///
/// 用具名字段而不是六个位置参数：`appid` / `mch_id` / `serial_no` / `v3_key` 都是同一种
/// 字符串，位置传参写反了照样编译，要等到网关返回 401 `SIGN_ERROR`（甚至更晚）才暴露。
///
/// ```no_run
/// # use wechat_pay_rust_sdk::pay::{ResponseVerify, WechatPay, WechatPayConfig};
/// let config = WechatPayConfig {
///     appid: "wx123".into(),
///     mch_id: "1900000001".into(),
///     private_key: std::fs::read_to_string("apiclient_key.pem")?,
///     serial_no: "5F2C…".into(),
///     v3_key: "0123456789abcdef0123456789abcdef".into(),
///     notify_url: "https://example.com/pay/notify".into(),
///     response_verify: ResponseVerify::Required,
/// };
/// let wechat_pay = WechatPay::from_config(config);
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Clone)]
pub struct WechatPayConfig {
    /// 商户 / 小程序 appid。
    pub appid: String,
    /// 商户号。
    pub mch_id: String,
    /// 商户 API 私钥（**PEM 内容本身**，不是文件路径）。
    pub private_key: String,
    /// 商户 API 证书序列号。
    pub serial_no: String,
    /// APIv3 密钥（32 字节，用于回调解密）。
    pub v3_key: String,
    /// 支付结果通知地址。
    pub notify_url: String,
    /// 出站应答的验签开关，默认 [`ResponseVerify::Required`]。
    ///
    /// 没有默认值是有意的：新增它是一次**行为变更**（默认强制验签），
    /// 让每处构造点显式写出这个选择，比留一个「忘了设就是关」的空洞安全。
    pub response_verify: ResponseVerify,
}

// 与 `WechatPay` 同理：derive 会把私钥与 APIv3 密钥原样打进日志。
impl std::fmt::Debug for WechatPayConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WechatPayConfig")
            .field("appid", &self.appid)
            .field("mch_id", &self.mch_id)
            .field("private_key", &"<redacted>")
            .field("serial_no", &self.serial_no)
            .field("v3_key", &"<redacted>")
            .field("notify_url", &self.notify_url)
            .finish()
    }
}

/// 出站应答是否验签。
///
/// 微信对**应答**（不只是回调）也做签名：`Wechatpay-Serial` / `-Timestamp` / `-Nonce` /
/// `-Signature` 四个头 + `{timestamp}\n{nonce}\n{body}\n` 的验签串，用的是同一批平台
/// 证书 / 微信支付公钥，因此回调验签的那套索引可以直接复用。
///
/// 默认（也是唯一推荐值）是 [`ResponseVerify::Required`]：不验签的应答等于把「这条应答
/// 真的来自微信」这件事交给 TLS 之外的运气 —— 一个能改应答的中间层可以伪造
/// 「订单不存在」「请求未受理」这类结论，进而诱导业务做出错误处置。
///
/// ⚠ [`ResponseVerify::Disabled`] **只用于**指向本地 mock 网关的离线测试，或网关确实
/// 无法回传 `Wechatpay-*` 头且暂时无法整改的场景；生产环境关闭等于放弃这一层防护。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseVerify {
    /// 强制验签（推荐）：2xx 应答缺任一签名头即 [`PayError::VerifyError`]。
    Required,
    /// 不验签：跳过应答签名校验，也不拉取平台证书。
    Disabled,
}

/// 刷新平台证书的单飞状态。
///
/// 用原子量而不是异步锁：`tokio` 只开了 `time`（没有 `sync`），而且刷新是「锁外拉取、
/// 锁内替换」，本来就不需要互斥保护整段流程 —— 只需要「同一时刻只有一个人去拉」。
#[derive(Debug, Default)]
pub(crate) struct RefreshState {
    /// 是否已有刷新在飞行中。
    pub(crate) in_flight: AtomicBool,
    /// 上一次由未知 serial 触发的**成功**刷新（unix 秒；0 表示从未）。
    ///
    /// 只在成功后写入（见 `refresh_platform_keys_for_unknown_serial`）：失败不该把限流窗口烧掉。
    pub(crate) last_unknown_serial_refresh: AtomicI64,
}

/// 刷新期间持有 `in_flight` 标记，`Drop` 时释放。
///
/// 用 RAII 而不是「拉完手动置回 false」：异步模式下这句话在 `.await` 之后，而调用方把
/// SDK 的 Future 包进 `tokio::time::timeout` / `select!`、或直接 abort 任务时，Future 会在
/// 那个 await 点被丢弃，手动释放的代码**永不执行** —— 标记会永久卡在 true，
/// 之后所有需要刷新的请求（含冷启动与 12 小时窗口）都会先等满、再失败。`Drop` 在取消路径
/// 上同样会跑。
pub(crate) struct RefreshGuard<'a>(&'a RefreshState);

impl<'a> RefreshGuard<'a> {
    /// 抢标记；返回 `None` 表示已有刷新在飞行中。
    pub(crate) fn acquire(state: &'a RefreshState) -> Option<Self> {
        state
            .in_flight
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| RefreshGuard(state))
    }
}

impl Drop for RefreshGuard<'_> {
    fn drop(&mut self) {
        self.0.in_flight.store(false, Ordering::SeqCst);
    }
}

/// 微信支付客户端。
///
/// 字段全部私有：`private_key` / `v3_key` 是能直接动钱的机密，`base_url` 决定你
/// 签好名的请求发往何处 —— 三者都不应被外部随手改写。要覆盖网关地址请用
/// [`WechatPay::with_base_url`]（主域名）或 [`WechatPay::with_base_urls`]（主 + 备）。
pub struct WechatPay {
    pub(crate) appid: String,
    pub(crate) mch_id: String,
    pub(crate) private_key: String,
    pub(crate) serial_no: String,
    pub(crate) v3_key: String,
    pub(crate) notify_url: String,
    pub(crate) base_url: String,
    /// 备域名（主 + 备轮换用）。空 = 只用主域名。见 [`WechatPay::with_base_urls`]。
    pub(crate) backup_base_urls: Vec<String>,
    /// 复用的 HTTP 客户端：连接池跨请求共享，避免每次请求重新建连 + TLS 握手。
    pub(crate) client: Client,
    /// 当前超时配置。改它需要连同重建 client，见 [`WechatPay::with_timeouts`]。
    pub(crate) timeouts: HttpTimeouts,
    /// 通用重试策略：只读查询与下单 / 关单。见 [`WechatPay::with_retry`]。
    pub(crate) retry: RetryPolicy,
    /// 退款专用重试策略，默认分钟级退避。见 [`WechatPay::with_refund_retry`]。
    pub(crate) refund_retry: RetryPolicy,
    /// 平台证书 / 微信支付公钥的索引：出站应答验签按 `Wechatpay-Serial` 选键。
    ///
    /// `RwLock` 而不是 `Mutex`：读（每次验签）远多于写（12 小时一次刷新）。
    /// ⚠ 锁内只做查表与一次 RSA 验签，**绝不跨 `.await` 持锁**（拉取在锁外完成，
    /// 拿写锁只是为了整体替换）。
    pub(crate) platform_keys: RwLock<PlatformKeys>,
    /// 密钥是否由调用方提供（公钥模式 / 固定证书来源）。
    ///
    /// 这两种来源下 `GET /v3/certificates` 里既没有这些 key（公钥模式的公钥不在平台证书
    /// 列表里），整体替换还会把它们抹掉 —— 所以置位后一律不自动拉取与替换。
    /// 用原子量而不是 `bool`：`set_platform_keys(&self, …)` 需要 `&self` 下改它。
    pub(crate) static_keys: AtomicBool,
    /// 平台证书刷新的单飞 / 限流状态。
    pub(crate) key_refresh: RefreshState,
    /// 等待并发平台证书刷新完成的预算（默认 [`DEFAULT_KEY_REFRESH_WAIT`]，2 秒）。
    /// 见 [`WechatPay::with_key_refresh_wait`]。
    pub(crate) key_refresh_wait: Duration,
    /// 出站应答是否验签，默认 [`ResponseVerify::Required`]。
    pub(crate) response_verify: ResponseVerify,
    /// 解析后的商户私钥（首次签名时解析一次）。
    ///
    /// PEM 解析会做密钥校验并预计算 CRT 参数，放在每次请求上既慢又只能 panic；
    /// 这里缓存下来，坏 PEM 则变成 [`PayError::SignError`] 返回给调用方。
    /// 用 `OnceLock` 而不是 `LazyLock`：初始值来自运行期配置（PEM 字符串），
    /// 不是声明期常量。
    pub(crate) parsed_key: std::sync::OnceLock<rsa::RsaPrivateKey>,
}

// `Debug` 手写而非 derive：derive 会把 `private_key` / `v3_key` 原样打进日志。
impl std::fmt::Debug for WechatPay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WechatPay")
            .field("appid", &self.appid)
            .field("mch_id", &self.mch_id)
            .field("private_key", &"<redacted>")
            .field("serial_no", &self.serial_no)
            .field("v3_key", &"<redacted>")
            .field("notify_url", &self.notify_url)
            .field("base_url", &self.base_url)
            .field("backup_base_urls", &self.backup_base_urls)
            .field("timeouts", &self.timeouts)
            .field("key_refresh_wait", &self.key_refresh_wait)
            .field("retry", &self.retry)
            .field("refund_retry", &self.refund_retry)
            // 这两项不是机密，而且是升级后排查的头号问题：
            // 「我是不是误关了验签」「现在到底认了哪几张证书」。
            .field("response_verify", &self.response_verify)
            .field("platform_keys", &self.platform_keys().serials())
            .finish()
    }
}

/// 回调与应答相关的能力：验签、解密。
///
/// 已为 [`WechatPay`] 实现。签名/解密用的密钥都来自 [`WechatPayTrait`] 的访问器。
pub trait PayNotifyTrait: WechatPayTrait {
    /// 用平台公钥验证 `"{timestamp}\n{nonce}\n{body}\n"` 的签名。
    ///
    /// ⚠ 只验签名，**不校验时间戳新鲜度**。做回调入口请用
    /// [`crate::notify`] 里的 `PlatformKeys::verify_notify`，它会额外做防重放检查。
    fn verify_signature<S>(
        &self,
        pub_key: &str,
        timestamp: S,
        nonce: S,
        signature: S,
        body: S,
    ) -> Result<(), PayError>
    where
        S: AsRef<str>,
    {
        let message = format!(
            "{}\n{}\n{}\n",
            timestamp.as_ref(),
            nonce.as_ref(),
            body.as_ref()
        );
        util::verify_rsa_sha256(pub_key, &message, signature.as_ref())
    }
    /// 解密回调里的 `resource`，并把明文解析成 [`WechatPayDecodeData`]。
    ///
    /// 三个参数都来自回调 body 的 `resource` 节点（`ciphertext` / `nonce` /
    /// `associated_data`）。⚠ 解密**不能替代验签**：先验签再解密。
    fn decrypt_paydata<S>(
        &self,
        ciphertext: S,
        nonce: S,
        associated_data: S,
    ) -> Result<WechatPayDecodeData, PayError>
    where
        S: AsRef<str>,
    {
        let plaintext = self.decrypt_bytes(ciphertext, nonce, associated_data)?;
        let data: WechatPayDecodeData = serde_json::from_slice(&plaintext)?;
        Ok(data)
    }
    /// 解密退款结果通知里的 `resource`，并解析成 [`WechatPayRefundDecodeData`]。
    ///
    /// 退款通知的字段与支付通知**不重合**（没有 `appid` / `trade_state`，多了
    /// `out_refund_no` / `refund_status`），所以走这个入口 —— 拿退款通知去调
    /// [`Self::decrypt_paydata`] 会以「缺字段」失败。
    fn decrypt_refund_paydata<S>(
        &self,
        ciphertext: S,
        nonce: S,
        associated_data: S,
    ) -> Result<WechatPayRefundDecodeData, PayError>
    where
        S: AsRef<str>,
    {
        let plaintext = self.decrypt_bytes(ciphertext, nonce, associated_data)?;
        let data: WechatPayRefundDecodeData = serde_json::from_slice(&plaintext)?;
        Ok(data)
    }
    /// 解密回调的 `resource` 节点（**先校验 `algorithm`**）并解析成 [`WechatPayDecodeData`]。
    ///
    /// 参数是整个 `resource` 节点（`WechatPayNotify::resource`），因此能在解密之前校验
    /// `algorithm` —— 不是 `AEAD_AES_256_GCM` 时返回明确的 [`PayError::DecryptError`]，
    /// 而不是一个方向错误的 GCM 解密失败。
    fn decrypt_notify(
        &self,
        resource: &WechatPayNotifySource,
    ) -> Result<WechatPayDecodeData, PayError> {
        resource.validate_algorithm()?;
        self.decrypt_paydata(
            resource.ciphertext.as_str(),
            resource.nonce.as_str(),
            resource.associated_data.as_deref().unwrap_or_default(),
        )
    }
    /// 退款结果通知版：字段与支付通知不重合（见 [`Self::decrypt_refund_paydata`]），
    /// 同样**先校验 `algorithm`** 再解密。
    fn decrypt_refund_notify(
        &self,
        resource: &WechatPayNotifySource,
    ) -> Result<WechatPayRefundDecodeData, PayError> {
        resource.validate_algorithm()?;
        self.decrypt_refund_paydata(
            resource.ciphertext.as_str(),
            resource.nonce.as_str(),
            resource.associated_data.as_deref().unwrap_or_default(),
        )
    }
    /// 用 APIv3 密钥做 AES-256-GCM 解密，返回明文（不解析）。
    ///
    /// 平台证书也是加密下发的，用它解出 PEM 证书后再交给
    /// [`crate::util::x509_to_pem`]。
    fn decrypt_bytes<S>(
        &self,
        ciphertext: S,
        nonce: S,
        associated_data: S,
    ) -> Result<Vec<u8>, PayError>
    where
        S: AsRef<str>,
    {
        let nonce: [u8; 12] = nonce
            .as_ref()
            .as_bytes()
            .try_into()
            .map_err(|_| PayError::DecryptError("nonce length must be 12".to_string()))?;
        let v3_key = self.v3_key();
        let ciphertext = util::base64_decode(ciphertext.as_ref())?;
        let cipher = Aes256Gcm::new_from_slice(v3_key.as_bytes())
            .map_err(|e| PayError::DecryptError(format!("invalid v3 key: {e}")))?;
        let payload = Payload {
            msg: ciphertext.as_slice(),
            aad: associated_data.as_ref().as_bytes(),
        };
        let plaintext = cipher
            .decrypt(&Nonce::from(nonce), payload)
            .map_err(|e| PayError::DecryptError(e.to_string()))?;
        Ok(plaintext)
    }
}

/// `wx.requestPayment` 的 `package` 前缀 —— 不同支付方式的取值规则不同。
///
/// 用枚举而不是再给一个字符串参数：`(prefix, prepay_id)` 都是字符串时写反了照样编译，
/// 只在前端拉起支付时才暴露。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackagePrefix {
    /// APP 支付：裸 `prepay_id`（多带 `prepay_id=` 前缀会被 APP 端拒绝）。
    Bare,
    /// JSAPI / 小程序 / 付款码：`prepay_id=xxx`。
    PrepayId,
}

impl PackagePrefix {
    /// 前缀字符串（[`PackagePrefix::Bare`] 为空串）。
    pub fn as_str(self) -> &'static str {
        match self {
            PackagePrefix::Bare => "",
            PackagePrefix::PrepayId => "prepay_id=",
        }
    }
}

/// 客户端的配置访问器与签名能力。
///
/// 访问器一律返回借用：它们只是暴露 [`WechatPay`] 的私有配置，交给调用方一份拷贝
/// 既没必要（`v3_key` 每次解密密文、`base_url` 每次重试都会多拷一份），也让密钥散落到
/// 更多地方。抽成 trait 是为了让 [`PayNotifyTrait`] 的默认方法能在不依赖具体类型的情况下
/// 复用。
pub trait WechatPayTrait {
    /// 商户 / 小程序 appid。
    fn appid(&self) -> &str;
    /// 商户号。
    fn mch_id(&self) -> &str;
    /// 商户 API 私钥（PEM 内容）。
    fn private_key(&self) -> &str;
    /// 商户 API 证书序列号。
    fn serial_no(&self) -> &str;
    /// APIv3 密钥（32 字节，用于回调解密）。
    fn v3_key(&self) -> &str;
    /// 支付结果通知地址。
    fn notify_url(&self) -> &str;
    /// **主**网关地址（备域名见
    /// [`WechatPay::with_base_urls`](crate::pay::WechatPay::with_base_urls)），默认
    /// `https://api.mch.weixin.qq.com`。
    fn base_url(&self) -> &str;
    /// 用商户私钥做 RSA-SHA256（PKCS#1 v1.5）签名，返回 base64。
    ///
    /// 私钥解析失败（PEM 不对、误填成文件路径）返回 [`PayError::SignError`]，
    /// **不会 panic** —— 那是配置问题，应当作为错误交给调用方。
    fn rsa_sign(&self, content: impl AsRef<str>) -> Result<String, PayError>;
    /// 当前 unix 时间戳（秒），用于签名串。
    fn now_timestamp(&self) -> String {
        util::now_unix_secs().to_string()
    }
    /// 随机串：UUID v4 去掉连字符后转大写，用于签名串与 Authorization 头。
    fn nonce_str(&self) -> String {
        Uuid::new_v4().to_string().replace("-", "").to_uppercase()
    }

    /// 构造给 `wx.requestPayment` 用的签名数据。
    ///
    /// `prefix` 由支付方式决定（见 [`PackagePrefix`]）；传错会导致前端拉起支付失败，
    /// 而失败现象只出现在客户端，很难排查。
    fn mut_sign_data(&self, prefix: PackagePrefix, prepay_id: &str) -> Result<SignData, PayError> {
        let app_id = self.appid().to_string();
        let now_time = self.now_timestamp();
        let nonce_str = self.nonce_str();
        let ext_str = format!("{}{prepay_id}", prefix.as_str());
        let signed_str =
            self.rsa_sign(format!("{app_id}\n{now_time}\n{nonce_str}\n{ext_str}\n"))?;
        Ok(SignData {
            app_id,
            sign_type: "RSA".into(),
            package: ext_str,
            nonce_str,
            timestamp: now_time,
            pay_sign: signed_str,
        })
    }
}

impl PayNotifyTrait for WechatPay {}

impl WechatPayTrait for WechatPay {
    fn appid(&self) -> &str {
        &self.appid
    }
    fn mch_id(&self) -> &str {
        &self.mch_id
    }
    fn private_key(&self) -> &str {
        &self.private_key
    }
    fn serial_no(&self) -> &str {
        &self.serial_no
    }
    fn v3_key(&self) -> &str {
        &self.v3_key
    }
    fn notify_url(&self) -> &str {
        &self.notify_url
    }
    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn rsa_sign(&self, content: impl AsRef<str>) -> Result<String, PayError> {
        sign::sha256_sign(self.parsed_private_key()?, content.as_ref())
    }
}

impl WechatPay {
    /// 覆盖默认网关地址（默认 `https://api.mch.weixin.qq.com`）。
    ///
    /// 主要用途是把请求指向本地 mock 服务做离线测试（见 `tests/offline.rs`）。
    /// ⚠ 生产代码绝不能让这个值受用户输入影响：你签好名的请求（含 openid、
    /// 订单信息）会被送到该地址。
    ///
    /// 只改**主**域名（已配置的备域名不受影响，下次重试仍可能落到备域名上 ——
    /// 测试里只想改主域名时请显式确认备域名列表为空）；要用官方推荐的备域名容灾见
    /// [`WechatPay::with_base_urls`]。
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// 配置**主 + 备**网关地址（官方「跨城冗灾」）。
    ///
    /// 第 1 次尝试用 `primary`，之后每次重试在 `[primary, backups…]` 里轮换
    /// （主 → 备 1 → 备 2 → 主 → …）。换域名重发是安全的：重试发的是**同一笔请求**
    /// （method / path / body 完全一致，每次尝试都会用新的时间戳与随机串重新签名，
    /// 见 `WechatPay::request`），而 host **不参与签名**（签名串第二行只有 path + query），
    /// 所以重放语义与在同一域名上重试完全一致；写接口「结果未知」的失败本来就不会重试，
    /// 因此也不存在「超时后换个域名重发写请求」的越界行为。
    ///
    /// ⚠ 与 [`WechatPay::with_base_url`] 同样的风险：这两个值只能来自你自己的配置，
    /// **绝不能**受用户输入影响 —— 你签好名的请求（含 openid、订单信息）会被送到这些地址。
    /// `backups` 为空时行为与只用主域名完全一致。两个 setter 的方向是**单向**的：
    /// `with_base_url` 只改主域名（备域名列表不动），而本方法会**同时覆盖主域名与备域名列表**
    /// （先配好备域名再调它，等于把两者一起换掉）。
    ///
    /// ```no_run
    /// # use wechat_pay_rust_sdk::pay::{ResponseVerify, WechatPay, WechatPayConfig};
    /// # let wechat_pay = WechatPay::from_config(WechatPayConfig {
    /// #     appid: "a".into(), mch_id: "b".into(), private_key: "c".into(),
    /// #     serial_no: "d".into(), v3_key: "e".into(), notify_url: "f".into(),
    /// #     response_verify: ResponseVerify::Required,
    /// # });
    /// let wechat_pay = wechat_pay.with_base_urls(
    ///     "https://api.mch.weixin.qq.com",
    ///     ["https://api2.mch.weixin.qq.com"],
    /// );
    /// ```
    #[must_use]
    pub fn with_base_urls<I>(mut self, primary: impl Into<String>, backups: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.base_url = primary.into();
        self.backup_base_urls = backups.into_iter().map(Into::into).collect();
        self
    }

    /// 第 `attempt` 次尝试（从 1 开始）使用的网关地址：主域名 → 备域名轮换 → 回到主域名。
    ///
    /// 没有配置备域名时恒为主域名 —— 与没有这个功能时的行为逐字节一致。
    pub(crate) fn gateway_for_attempt(&self, attempt: u32) -> &str {
        if self.backup_base_urls.is_empty() {
            return &self.base_url;
        }
        let index = (attempt.saturating_sub(1) as usize) % (self.backup_base_urls.len() + 1);
        if index == 0 {
            &self.base_url
        } else {
            &self.backup_base_urls[index - 1]
        }
    }

    /// 覆盖应答验签开关（默认 [`ResponseVerify::Required`]）。
    ///
    /// ⚠ [`ResponseVerify::Disabled`] 只适用于指向本地 mock 网关的离线测试：
    /// 关闭后「应答真的来自微信」这件事不再被校验，能改应答的中间层可以伪造
    /// 「订单不存在」「请求未受理」这类结论。网关（代理 / CDN）过滤了
    /// `Wechatpay-*` 头时应当修网关，而不是关验签。
    #[must_use]
    pub fn with_response_verify(mut self, mode: ResponseVerify) -> Self {
        self.response_verify = mode;
        self
    }

    /// 当前的应答验签开关。
    pub fn response_verify(&self) -> ResponseVerify {
        self.response_verify
    }

    /// 覆盖「等待并发平台证书刷新完成」的预算（默认 [`DEFAULT_KEY_REFRESH_WAIT`]，2 秒）。
    ///
    /// 平台证书刷新是**单飞**的：同一时刻只有一个请求去打 `/v3/certificates`，其余请求轮询
    /// 等待它结束。等超预算的请求会拿到 [`PayError::UnknownPlatformSerial`]（消息里带预算毫秒数）。
    /// 并发冷启动多、或证书接口偶发慢时调大它可以少一轮业务失败；传 `Duration::ZERO`
    /// 表示完全不等待（一发现有刷新在飞行中就立即失败）。默认 2 秒与旧行为一致。
    #[must_use]
    pub fn with_key_refresh_wait(mut self, wait: Duration) -> Self {
        self.key_refresh_wait = wait;
        self
    }

    /// 当前的证书刷新等待预算。
    pub fn key_refresh_wait(&self) -> Duration {
        self.key_refresh_wait
    }

    /// 覆盖 HTTP 超时配置（阈值见 [`HttpTimeouts`] 的默认值）。
    ///
    /// 会重建 HTTP 客户端，因此请在启动阶段调用，不要在每请求路径上调用。
    ///
    /// 只改其中一项：
    ///
    /// ```no_run
    /// # use std::time::Duration;
    /// # use wechat_pay_rust_sdk::pay::{HttpTimeouts, ResponseVerify, WechatPay, WechatPayConfig};
    /// # let wechat_pay = WechatPay::from_config(WechatPayConfig {
    /// #     appid: "a".into(), mch_id: "b".into(), private_key: "c".into(),
    /// #     serial_no: "d".into(), v3_key: "e".into(), notify_url: "f".into(),
    /// #     response_verify: ResponseVerify::Required,
    /// # });
    /// let wechat_pay = wechat_pay.with_timeouts(HttpTimeouts {
    ///     request: Duration::from_secs(30),
    ///     ..HttpTimeouts::default()
    /// });
    /// ```
    #[must_use]
    pub fn with_timeouts(mut self, timeouts: HttpTimeouts) -> Self {
        self.client = build_client(timeouts);
        self.timeouts = timeouts;
        self
    }

    /// 当前的超时配置。
    pub fn timeouts(&self) -> HttpTimeouts {
        self.timeouts
    }

    /// 覆盖通用重试策略（只读查询与下单 / 关单）。
    ///
    /// 默认是 [`RetryPolicy::default`]：最多 3 次尝试，200ms 起步的指数退避 + 抖动。
    /// **策略只控制次数与间隔；「该不该重试」由失败分类决定**，详见 [`crate::retry`]。
    ///
    /// ```no_run
    /// # use wechat_pay_rust_sdk::pay::{ResponseVerify, WechatPay, WechatPayConfig};
    /// # use wechat_pay_rust_sdk::retry::RetryPolicy;
    /// # let wechat_pay = WechatPay::from_config(WechatPayConfig {
    /// #     appid: "a".into(), mch_id: "b".into(), private_key: "c".into(),
    /// #     serial_no: "d".into(), v3_key: "e".into(), notify_url: "f".into(),
    /// #     response_verify: ResponseVerify::Required,
    /// # });
    /// // 只改次数，其余沿用默认
    /// let wechat_pay = wechat_pay.with_retry(RetryPolicy {
    ///     max_attempts: 5,
    ///     ..RetryPolicy::default()
    /// });
    /// // 或者完全关掉
    /// let wechat_pay = wechat_pay.with_retry(RetryPolicy::disabled());
    /// ```
    #[must_use]
    pub fn with_retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = policy;
        self
    }

    /// 覆盖**退款专用**的重试策略。
    ///
    /// 默认是 [`RetryPolicy::for_refund`]：最多 2 次尝试，首次退避 **60 秒**。
    /// 依据是官方对退款重试的节奏要求（「间隔 1 分钟」）以及该接口在失败时报错限流
    /// 只有 6QPS —— 秒级退避打过去基本是白打，还会加重限流。
    ///
    /// ⚠ **分钟级退避意味着阻塞**：一次退款调用最坏会挂住约 1 分钟
    /// （退避 + 请求超时）。同步调用链里扛不住的话，用
    /// `with_refund_retry(RetryPolicy::disabled())` 关掉，改由业务侧异步重试 ——
    /// 分钟级节奏本来就是异步任务更自然的载体。
    #[must_use]
    pub fn with_refund_retry(mut self, policy: RetryPolicy) -> Self {
        self.refund_retry = policy;
        self
    }

    /// 用**微信支付公钥**验签（公钥模式）。
    ///
    /// 参数顺序：`public_key_id`（商户平台下载公钥时给出的 ID，形如
    /// `PUB_KEY_ID_0000000000000024101100397200006`）、`public_key_pem`（公钥 PEM 内容）。
    /// 必须与微信在 `Wechatpay-Serial` 头里回传的值一致，否则选不到键。
    ///
    /// ⚠ 公钥模式下的公钥**不在** `GET /v3/certificates` 里（要从商户平台下载、自行更新），
    /// 所以这个客户端进入**静态密钥模式**：不会自动拉取、也不会自动替换密钥 ——
    /// 平台证书列表里没有这张公钥，拉取后的替换会把它抹掉。轮换要自己更新
    /// （[`WechatPay::set_platform_keys`] 再设一次，或重建客户端）。
    ///
    /// ⚠ **微信支付公钥 / 平台证书的灰度切换期请改用
    /// [`WechatPay::add_static_platform_key`]**：它注入公钥但**不**切模式，因此同一个客户端
    /// 既能验公钥签的、也能验平台证书签的（灰度期两种都可能出现）。
    #[must_use]
    pub fn with_platform_public_key(
        self,
        public_key_id: impl Into<String>,
        public_key_pem: impl Into<String>,
    ) -> Self {
        let mut keys = PlatformKeys::new();
        keys.insert(public_key_id, public_key_pem);
        self.set_platform_keys(keys);
        self
    }

    /// 向**静态密钥表**注入一把调用方提供的公钥（微信支付公钥 / 固定证书来源），
    /// **不改变模式**。
    ///
    /// 与 [`WechatPay::with_platform_public_key`] / [`WechatPay::set_platform_keys`] 的区别：
    /// 不会切到静态密钥模式，平台证书的自动拉取 / 12 小时刷新 / 未知 serial 刷新照旧 ——
    /// 同一个客户端因此能**同时**验证「静态公钥」与「轮换中的平台证书」签名的回调与应答，
    /// 这正是官方公钥/证书**灰度切换期**必须的行为（切换期间同一商户的报文可能由两者随机签名）。
    ///
    /// 选键顺序：静态表优先；同一 serial 两表都有时静态表获胜（轮换表那条只是被遮蔽，不删除）。
    ///
    /// PEM **当场解析**：坏 PEM（例如误传了证书 PEM）在这里就报
    /// [`PayError::VerifyError`]，不等到第一笔回调。返回 `Ok(true)` 表示覆盖了静态表里
    /// 同 serial 的旧条目。
    ///
    /// 注入本身**不会**触发任何网络请求：平台证书仍在需要时（未知 serial）才拉取。
    pub fn add_static_platform_key(
        &self,
        serial: impl Into<String>,
        public_key_pem: impl Into<String>,
    ) -> Result<bool, PayError> {
        let public_key_pem = public_key_pem.into();
        let parsed = Arc::new(crate::util::parse_rsa_public_key(&public_key_pem)?);
        Ok(self
            .platform_keys_write()
            .insert_parsed(serial.into(), public_key_pem, parsed))
    }

    /// 当前平台密钥索引的快照（`serial_no -> 公钥 PEM`，**两张表合并**）。
    ///
    /// 用途是排查「到底认了哪几张证书」（轮换期正常是 2 张，加上注入的公钥可能更多），
    /// 不适合放进每请求路径：每次调用都会拷一份索引。
    /// 回调验签用的就是这份快照，因此它同时包含注入的静态公钥与自动拉取的平台证书。
    pub fn platform_keys(&self) -> PlatformKeys {
        self.platform_keys_read().clone()
    }

    /// 直接设置平台密钥索引，并进入**静态密钥模式**（测试、固定证书、自建密钥源）。
    ///
    /// 静态模式下：不会自动拉取 `/v3/certificates`、不会用拉取结果替换你设置的索引。
    /// 代价是轮换要你自己更新 —— 对公钥模式（[`WechatPay::with_platform_public_key`]）
    /// 而言这是唯一正确的语义，因为公钥根本不在平台证书列表里。
    ///
    /// 本调用**整体替换两张表**：传进来的索引就是全部可信材料，之前自动拉取的平台证书
    /// 也会被清掉（与「调用方负责」的语义一致）。只注入静态表而不影响轮换表请用
    /// [`WechatPay::add_static_platform_key`]。
    pub fn set_platform_keys(&self, keys: PlatformKeys) {
        // ⚠ 置位与写入必须在**同一个写锁临界区**内：安装侧（`fetch_and_install_keys_once`）
        // 是在写锁内复查 `auto_refresh_keys()` 的，若这里先放锁再置位，就会出现
        // 「索引里已经是调用方的密钥、但标记还说不是静态模式」的窗口 ——
        // 此刻一次飞行中的拉取会通过复查并把用户密钥整体覆盖，
        // 而静态模式下不会再有下一次自动拉取 ⇒ 公钥模式永久失效。
        let mut guard = self.platform_keys_write();
        self.static_keys.store(true, Ordering::SeqCst);
        *guard = keys;
    }

    /// 读锁；锁中毒不 panic（中毒只说明持锁线程 panic 过，索引本身仍然可用）。
    pub(crate) fn platform_keys_read(&self) -> std::sync::RwLockReadGuard<'_, PlatformKeys> {
        self.platform_keys
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 写锁；中毒处理同 [`WechatPay::platform_keys_read`]。
    pub(crate) fn platform_keys_write(&self) -> std::sync::RwLockWriteGuard<'_, PlatformKeys> {
        self.platform_keys
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 是否允许自动拉取平台证书（静态密钥模式下为 `false`）。
    pub(crate) fn auto_refresh_keys(&self) -> bool {
        !self.static_keys.load(Ordering::SeqCst)
    }

    /// 当前的通用重试策略。
    pub fn retry_policy(&self) -> RetryPolicy {
        self.retry
    }

    /// 当前退款专用的重试策略。
    pub fn refund_retry_policy(&self) -> RetryPolicy {
        self.refund_retry
    }

    /// 按请求的重放语义挑出该用哪个策略。
    ///
    /// 显式列出每个 [`RequestKind`] 而不是用 `_`：新增接口类型时编译器会提醒这里
    /// 也要做出决定，而不是悄悄套用通用策略。
    pub(crate) fn policy_for(&self, kind: RequestKind) -> RetryPolicy {
        match kind {
            RequestKind::Refund => self.refund_retry,
            RequestKind::Read | RequestKind::Write => self.retry,
        }
    }

    /// 取已解析的商户私钥；首次使用时解析并缓存。
    fn parsed_private_key(&self) -> Result<&rsa::RsaPrivateKey, PayError> {
        if let Some(key) = self.parsed_key.get() {
            return Ok(key);
        }
        let key = sign::parse_private_key(&self.private_key)?;
        Ok(self.parsed_key.get_or_init(|| key))
    }

    /// 用商户配置构造客户端。
    ///
    /// `private_key` 需要 **PEM 内容本身**（不是文件路径），`v3_key` 必须是 32 字节。
    /// 私钥直到第一次签名才解析，解析失败会作为 [`PayError::SignError`] 返回 ——
    /// 也就是说构造不会 panic，坏配置在第一次请求时才暴露。
    /// 网关默认 `https://api.mch.weixin.qq.com`，用 [`WechatPay::with_base_url`]
    /// 覆盖主域名、[`WechatPay::with_base_urls`] 配置主 + 备域名。
    ///
    /// ```no_run
    /// # use wechat_pay_rust_sdk::pay::{ResponseVerify, WechatPay, WechatPayConfig};
    /// let wechat_pay = WechatPay::from_config(WechatPayConfig {
    ///     appid: "wx123".into(),
    ///     mch_id: "1900000001".into(),
    ///     private_key: std::fs::read_to_string("apiclient_key.pem")?,
    ///     serial_no: "5F2C…".into(),
    ///     v3_key: "32 字节的 APIv3 密钥".into(),
    ///     notify_url: "https://example.com/pay/notify".into(),
    ///     response_verify: ResponseVerify::Required,
    /// });
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn from_config(config: WechatPayConfig) -> Self {
        let timeouts = HttpTimeouts::default();
        Self {
            appid: config.appid,
            mch_id: config.mch_id,
            private_key: config.private_key,
            serial_no: config.serial_no,
            v3_key: config.v3_key,
            notify_url: config.notify_url,
            base_url: "https://api.mch.weixin.qq.com".to_string(),
            backup_base_urls: Vec::new(),
            client: build_client(timeouts),
            timeouts,
            retry: RetryPolicy::default(),
            refund_retry: RetryPolicy::for_refund(),
            platform_keys: RwLock::new(PlatformKeys::new()),
            static_keys: AtomicBool::new(false),
            key_refresh: RefreshState::default(),
            key_refresh_wait: DEFAULT_KEY_REFRESH_WAIT,
            response_verify: config.response_verify,
            parsed_key: std::sync::OnceLock::new(),
        }
    }

    /// 初始化全局 `tracing` 订阅器，把 crate 的 `debug!` 输出打到标准输出。
    ///
    /// 仅在 `debug-print` feature 下存在。⚠ 它会记录请求体、Authorization 头与被签名的
    /// 原文（含 openid、订单号、金额），**生产环境不要开启**。
    #[cfg(feature = "debug-print")]
    pub fn open_debug(&self) {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_line_number(true)
            .init();
    }

    /// 从环境变量构造客户端：`WECHAT_APPID` / `WECHAT_MCH_ID` / `WECHAT_PRIVATE_KEY` /
    /// `WECHAT_SERIAL_NO` / `WECHAT_V3_KEY` / `WECHAT_NOTIFY_URL`。
    ///
    /// ⚠ 缺任何一个变量都会 panic（内部用 `expect`）。
    /// ⚠ `WECHAT_PRIVATE_KEY` 是 **PEM 文件路径**，不是密钥内容 —— 这里会读该文件。
    pub fn from_env() -> Self {
        let appid = std::env::var("WECHAT_APPID").expect("WECHAT_APPID not found");
        let mch_id = std::env::var("WECHAT_MCH_ID").expect("WECHAT_MCH_ID not found");
        let private_key =
            std::env::var("WECHAT_PRIVATE_KEY").expect("WECHAT_PRIVATE_KEY not found");
        let serial_no = std::env::var("WECHAT_SERIAL_NO").expect("WECHAT_SERIAL_NO not found");
        let v3_key = std::env::var("WECHAT_V3_KEY").expect("WECHAT_V3_KEY not found");
        let notify_url = std::env::var("WECHAT_NOTIFY_URL").expect("WECHAT_NOTIFY_URL not found");
        let private_key = std::fs::read_to_string(private_key).expect("read private key error");
        Self::from_config(WechatPayConfig {
            appid,
            mch_id,
            private_key,
            serial_no,
            v3_key,
            notify_url,
            // 环境变量这条路没有「关掉验签」的开关：默认必须是强制的。
            response_verify: ResponseVerify::Required,
        })
    }

    /// 组装签名请求头。
    ///
    /// ⚠ 每次调用都会生成新的 `timestamp` / `nonce_str` 并重新签名 —— 重试必须如此：
    /// 复用旧签名可能撞上 5 分钟的签名有效期窗口。
    pub(crate) fn build_header(
        &self,
        method: HttpMethod,
        url: impl AsRef<str>,
        body: impl AsRef<str>,
    ) -> Result<HeaderMap, PayError> {
        let method = method.to_string();
        let url = url.as_ref();
        let body = body.as_ref();
        let timestamp = util::now_unix_secs();
        let nonce_str = Uuid::new_v4().to_string().replace("-", "").to_uppercase();
        let message = format!(
            "{}\n{}\n{}\n{}\n{}\n",
            method, url, timestamp, nonce_str, body,
        );
        debug!("rsa_sign message: {}", message);
        let signature = self.rsa_sign(message)?;
        let authorization = format!(
            "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",signature=\"{}\",timestamp=\"{}\",serial_no=\"{}\"",
            self.mch_id, nonce_str, signature, timestamp, self.serial_no,
        );
        debug!("authorization: {}", authorization);
        // 静态字面量用 from_static；Authorization 由配置插值而来，可能含不能进 HTTP 头的
        // 字符（例如 serial_no 带了个换行）—— 那是配置错误，应当返回 Err 而不是 panic。
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let chrome_agent = "Mozilla/5.0 (Linux; Android 10; Redmi K30 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/86.0.4240.198 Mobile Safari/537.36";
        headers.insert(
            USER_AGENT,
            reqwest::header::HeaderValue::from_static(chrome_agent),
        );
        headers.insert(
            AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&authorization).map_err(|e| {
                PayError::SignError(format!("商户号 / 序列号含非法 HTTP 头字符: {e}"))
            })?,
        );
        headers.insert(
            CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        Ok(headers)
    }
}

#[cfg(test)]
mod tests {
    use crate::pay::{PayNotifyTrait, ResponseVerify, WechatPay, WechatPayConfig, WechatPayTrait};
    use dotenvy::dotenv;
    use rsa::pkcs8::DecodePublicKey;
    use rsa::sha2::{Digest, Sha256};
    use rsa::{Pkcs1v15Sign, RsaPublicKey};
    use tracing::debug;

    #[inline]
    fn init_log() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_line_number(true)
            .init();
    }

    #[test]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    fn test_rsa_sign() {
        init_log();
        let private_key_path = "./apiclient_key.pem";
        let private_key = std::fs::read_to_string(private_key_path).unwrap();
        let wechat_pay = WechatPay::from_config(WechatPayConfig {
            appid: String::new(),
            mch_id: String::new(),
            private_key,
            serial_no: String::new(),
            v3_key: String::new(),
            notify_url: String::new(),
            response_verify: ResponseVerify::Required,
        });
        let sign_str = wechat_pay.rsa_sign("hello").expect("签名必须成功");
        debug!("sign_str: {}", sign_str);
    }

    /// 支付回调参数解密
    #[test]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    fn test_decrypt_paydata() {
        init_log();
        dotenv().ok();
        let associated_data = "transaction";
        let nonce = "gZiqzlfayUu2";
        let ciphertext = "pCidqdiS5IIj5f9Pw9j69zuzu8l8IxcPCkfsTBKzna4gqZztNAqTMUY/Ai0rtj8qhaX0naYZF3a2lRid/ofK/83MNv+Neb5+w/0+UOO9nLNJvIFy3oFeMf2PTbp6tgDE35T5AoP9iKQ+1VkXTiUdRxzFoRx6/LfBzHmeuVEDHKScRqjrf6NdxuDDD0ciCQaiHmb18Y0BRZdfNxWTAC83Rar5yTX2NNZPBtGdFDG3yAK2I3Vp7ZKLeMa92ecExNGwHrdJ+HxWw66IIdwVqJLlNmTG0c5zUpSc8yovnaJi1Wv/TC7Tm5NzcwdHsdRE110tIWFbvNmIzIIb+3P33JFWmaXXb1VVDC43DqtlplttYwL6H3kU0ABgHMMbccTwYmP4cSY8BCAL01754nqipxWogEC/la9iQiw85+rLRo/Ny9k3mp8n35D6bDNtS1LiaslbLM92ZbfKeglTg54F/R1l5xWolAVpx8iTz8Oc+XJClXdWr8j5poyh8zK2/RrXPRfr+8s2/oGeGvdaqJbN/LviYcCMDbXU9pKDScWlSi4akxfJu0EatPDvFEbn5DYRQnn5v6wCeesYkEL+wiFCAIs=";
        let wechat_pay = WechatPay::from_env();
        let data = wechat_pay
            .decrypt_paydata(ciphertext, nonce, associated_data)
            .unwrap();
        debug!("data: {:#?}", data);
    }

    /// 把微信支付平台证书序列号转换成16进制字符串
    /// ```text
    /// -----BEGIN CERTIFICATE-----
    /// xxxxasdfaskd内容fjalskdfjasdf
    /// -----END CERTIFICATE-----
    /// ```
    #[test]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    fn test_decrypt_certificates() {
        init_log();
        dotenv().ok();
        let associated_data = "certificate";
        let nonce = "bf003ed52d71";
        let ciphertext = "HE9tL+x8Mag2627GPRXBmaQxZPVhAm3f2UoxgHvVW+m6eN0vq6ggFf4UsaQ8ifeGKwjhj9M6ObREHNogrT5JlEDV4Mfg8pAcLNvKUnbQZeBFKtp8kXPy0KFGhfSWcMZ4+HyfAUkgqLpdRUuNpG3gSJptnfrbJktdtYifkDOcei+1ncq8x+aWCXkFw8l9xBSN8MVSf66TiKyuPD/QCKYbD92HHfmDHk2b8J+BKyISDlQTlKjpb9M01EnuPsIXi4Rww1YzZP8XDruRTFxxDxGmk74tu1cjGXzTIcNmFu85eHbLWENvoLttl/4cKLJ8w49PuCyrREACz1YeAOscEHsqYHaQ0VE2N/8J0wCBuQa+AVD6ra59lmCxRJOVfgQNTShxonA6uCfaPGtyg+5qlwYTESnSdIy2ODlXaOfzMT5N7/actJsEf2C7RJXTPWn79M5slVfE3gOh9aR3mJaEMFM9KZywqv4OT0OI9mpLqRLAV/QCkJ0q2SKCcZyIuLa+VAPVS5Rh2feQkP40iizvVPN68YMOAmVgMBYLaxehGnetT2UylTlqsov35hsbfKOEN5ArSr7y4xoTjW5BV4S0s2IDzHTHWQpMlTxJ59/sgMoq8+m8vezJ0W4AZubwG/iSQ+/tzv1CXAVUgMO8ZqEALpGiROVq+9hdD5a0UB6cGuOTw9OiQHLSn1M4zV2jWDLQSZ+Q8KFhTpMibnvdLFmC09k26K75VcACsNPSa9U+nvP9sp3H7a39Y9BXjIz8/Yd707Y8h76MpEWLsVTn7FvRWwaCi4vxZN/LMRh9KTLNffQcb5amoDYKVSr5BTshdM7EosNwQmGenNnAFlNE/mabXSIz+FC3gMlDbxVvaoB5vOLB/YHrqfoLMEtYGm2HGqjppLmkbNM/R/6NIDFe+jaXZPWh9Bt8F4blihJnbEsZlC/w0/2OylTUsjRipG443XhOLEZJgD54KOnQdpqDah+AW2tPq5V9528ePK5xJzZ33MB3kjgnmljaF5cVbgUcCp5e8N+zvFVoyltsYMNNrtOan0Zfpsj9hNPnUVKLEnsjGXyfpBazRKoOOrPK5MImLUt/JblT+PFZ1oSrQE1IRRfF88yaUYwY2qk3pTrqBY676hOIUesWwuN4CSm28lLu/VarJaY0iLKuoGF0eikGFnAae1BIuFxvDUc3C+vC4GXUFn9jr3PZQcGJuI3MbEk8xGFWcU8UBU2wWhRu5lIgFSX5krbe+FWmRSjl6Rc3s7HZi5Xa8RiRuN1bOcnhVYkNYXy1fg7lXoopWJJPgtMO/+DDTNGQe8G0UgQxy+OK0urlhtzGQjVhF838i0heG4JV+OWUKj/Qvoj/dxVVfIbfroupkg8GvMmn0Cq+nAuo0D0fvhQshDmRsL/a006piEiLthruMn/gymk8cccMVvzn+DxYfYH/WX2UKZ235hPynVLUo8FBBedVTQK3JuJHCT4Kz0lL28KRLpE+lW3/bzG9s0Bly7/h1BF5Xunv4TWYhMFseWGMRIiKR7HxMSXbD4Q1PQJrZt/DtP3JbPURfc6fuYPIb7iuka0kDkPGSCV2uCpzjVHZXYQWrDhFv7LWi4SUw+2mCZLsLR6kesexb5bBOMVRxnA/5WmYVp73WzXar28CW3l0WCccGL/EdVdhrx09RoW5GSy9zcjbyGhwZQuzZECbf/wCpd26YlMTzFP0bqfL/QJ4g32TX8XweyhTPRI7FX1Bg8x85GJYG/bvecR40lDj4A0WKGnVbic3e7LQpDi/BP9adDBxx3Nl0iCN9BUlMx6ypNmrQWHwQXgmPwapyByK0FHjmf0u7hExZ7+xMa9/DPo2YPJdAY6zuHlNUIXLEVa9/VrclsYbyGkeohFGsMgY5MIA0ZF5FFxEOQ31gtNgQiGIVywSGJS8L+qB3tDc07O8hMxCY9wKPP2ua0MkkKQ7O6cr3W1DxNsd9NCbENDW4zNHzT+4pafS1TFaEy0nHI/wIQEyJlXD";
        let wechat_pay = WechatPay::from_env();
        let data = wechat_pay
            .decrypt_bytes(ciphertext, nonce, associated_data)
            .unwrap();
        debug!("data: {}", String::from_utf8(data).unwrap());
    }

    #[test]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    fn test_verify_sign() {
        init_log();
        dotenv().ok();
        let wechat_pay = WechatPay::from_env();
        let pub_key = std::fs::read_to_string("pub.pem").unwrap();
        let wechatpay_signature = "mFgmwXAKL3YJj34b7f+cUG3vkW09TiXU4lOSzCbvWFtvyLTb5WiyfAiVXZmMB17Qh9gDVkqboO97zfIYfv+AVdxj3GQljWlW+vE1Ujn2uxiFld6bWwz8Znk+833ruzZ8mAIaqLEjI/HKuVPdTj4LFzh/EO+gEMR6WDXr+7cZV7D3qUTXuO26fHLe0PmleDziG8SPgYjihK1ztF3Os0NhvL5tQMM8LKDOMzO3kxSr/TqTBtsB/OnuP2mH8yaSUeYeTpGStYvSw8KVi+gk6VnrlkVmdFh3DDXY60GCzCZ8zPl12RmzZbBRSK8ocVrzs4tuqRa5Euk3cDIA6qHqS8hyBQ==";
        let body = r#"{"id":"29a61973-babf-599a-966d-6bcdcf17360c","create_time":"2024-01-12T21:39:44+08:00","resource_type":"encrypt-resource","event_type":"TRANSACTION.SUCCESS","summary":"支付成功","resource":{"original_type":"transaction","algorithm":"AEAD_AES_256_GCM","ciphertext":"5ZfDK+LRJakAkC7kdHKRzCu5WZ0JFC2qSwP4InWNFeUnY0uaOnzfCjiqhDTFYyP4ywxuLxPUOiVI3WT6CcU0NNqbadTQ5XzjVuKLxYSnOYCFULltIrfsT/mUF4VW+xBMgSgG4+ZdzhRXVr+AzihDKFjw2p1iCtLYz9emgToctygNBtV6JDEI2BnCoiEM7qyIU1ALv5IsufQHDQqzjYXd16OD3i6O8UeSE2GOd4ifmQrAKGKalwWPECI73/qTFoAcLcgbhhn1TeSEaHoF7xceDmkL9AGlC21pBwYWoibTgqdlDJiz3IctrCzH6PPXD8XcApEj4A3ByyPjaNs6HxaJGzEHYGUkyM2/b7SzZIzqlBmNRZYFvBC0BOwoktyxrIhg3bKSbYtDYt1+8lMaYIJW6Dgq9GjG6pxAVrYULt8sk8cKZ+OrK9iXHZI11pYyK9YwWJLXbs6GyjMdDxhaGilF9csK8ZSsKzUjvlcLCjboCFX6nuHvCbswchYchQhTeitKDKG3/q+4snY183dBA6rXBHKQduqc1vXRR6odMcU1Evvy5mKnDTDELlI6mqvBtJ10XNED5O43ga5ZAODxYoU=","associated_data":"transaction","nonce":"uaGeNnBYNjl7"}}"#;
        let wechatpay_timestamp = "1705066785";
        let wechatpay_nonce = "Jh9oPZelCJIQeQ47kz4stzvDKpLEUhCX";
        wechat_pay
            .verify_signature(
                pub_key.as_str(),
                wechatpay_timestamp,
                wechatpay_nonce,
                wechatpay_signature,
                body,
            )
            .unwrap();
    }

    #[test]
    #[ignore = "需要真实商户凭证 / 公网 / 未入库的 PEM fixture；用 `cargo test -- --ignored` 显式运行"]
    fn test_pay_verify_sign() {
        let signature = std::fs::read("signature.txt").unwrap();
        let message = std::fs::read("message.txt").unwrap();
        let pub_key = RsaPublicKey::read_public_key_pem_file("pub.pem").unwrap();

        let hashed = rsa::sha2::Sha256::new()
            .chain_update(message.as_slice())
            .finalize();
        let scheme = Pkcs1v15Sign::new::<Sha256>();
        pub_key
            .verify(scheme, &hashed, signature.as_slice())
            .expect("签名验证失败");
        // 方法2：错误
        // let signature = Signature::try_from(signature.as_slice()).expect("签名解析失败");
        // let verifying_key: VerifyingKey<Sha256> = VerifyingKey::from(pub_key.clone());
        // verifying_key
        //     .verify(message.as_slice(), &signature)
        //     .expect("签名验证失败")
    }

    #[test]
    fn gateway_rotation_cycles_through_the_primary_and_backups() {
        let base = WechatPay::from_config(WechatPayConfig {
            appid: String::new(),
            mch_id: String::new(),
            private_key: String::new(),
            serial_no: String::new(),
            v3_key: String::new(),
            notify_url: String::new(),
            response_verify: ResponseVerify::Required,
        });
        // 没有备域名：恒为主域名（既有行为）
        assert_eq!(base.gateway_for_attempt(1), "https://api.mch.weixin.qq.com");
        assert_eq!(base.gateway_for_attempt(3), "https://api.mch.weixin.qq.com");

        let rotated =
            base.with_base_urls("https://primary", ["https://backup1", "https://backup2"]);
        assert_eq!(rotated.gateway_for_attempt(1), "https://primary");
        assert_eq!(rotated.gateway_for_attempt(2), "https://backup1");
        assert_eq!(rotated.gateway_for_attempt(3), "https://backup2");
        assert_eq!(
            rotated.gateway_for_attempt(4),
            "https://primary",
            "轮换必须回到主域名"
        );
    }
}
