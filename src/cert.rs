//! 平台证书 / 微信支付公钥的按键索引（**两张表**:调用方注入的静态密钥 + 自动轮换的平台证书）。
//!
//! 微信平台证书**轮换期会同时下发新旧两张且都在有效期内**，验签必须按请求头
//! `Wechatpay-Serial` 选择对应的密钥。写死单张证书的写法在轮换时会直接导致回调
//! 验签全部失败 —— 也就是订单不发货。
//!
//! 微信支付公钥与平台证书的**灰度切换期**又要求反过来：同一商户的回调/应答可能由**公钥**
//! 或**平台证书**签名，两种都得认。因此 [`PlatformKeys`](crate::cert::PlatformKeys) 里是两张表：
//! 调用方用 `insert` / [`WechatPay::add_static_platform_key`](crate::pay::WechatPay::add_static_platform_key)
//! 注入的固定公钥，以及 SDK 自动拉取的平台证书；选键**静态表优先**。
//!
//! 本模块提供：
//!
//! * [`PlatformKeys`](crate::cert::PlatformKeys) —— `serial_no -> 公钥 PEM` 的两表索引，
//!   支持按 serial 选键；
//! * [`WechatPay::fetch_platform_keys`](crate::pay::WechatPay::fetch_platform_keys) ——
//!   拉取 `GET /v3/certificates`、逐张解密并建索引（**无状态**，返回给调用方）；
//! * [`WechatPay::refresh_platform_keys`](crate::pay::WechatPay::refresh_platform_keys) ——
//!   拉取并**只替换轮换表**（注入的静态密钥不受影响；出站应答验签用的就是同一份索引）。
//!
//! 轮换表由 SDK 自己维护：每次请求前按 12 小时窗口判断是否该刷新（冷启动时就是首次拉取），
//! 遇到未知 `Wechatpay-Serial` 会**再刷新一次并重验**；已注入静态密钥且从未拉取过时不预拉
//! （改为按需，见 [`PlatformKeys::needs_refresh`](crate::cert::PlatformKeys::needs_refresh)）。
//! 回调验签请用手里的 [`PlatformKeys`](crate::cert::PlatformKeys)：`PlatformKeys::verify_notify`，
//! 一旦拿到
//! [`PayError::UnknownPlatformSerial`](crate::error::PayError::UnknownPlatformSerial)
//! 就**立即重新拉取**再重试。

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;
use rsa::RsaPublicKey;

use crate::async_impl::pay::{ResponseCheck, sleep_for};
use crate::error::PayError;
use crate::macros::debug;
use crate::pay::{PayNotifyTrait, RefreshGuard, ResponseVerify, WechatPay};
use crate::request::HttpMethod;
use crate::response::{Certificate, CertificateResponse, EncryptCertificate};
use crate::retry::RequestKind;
use crate::util;

#[cfg(feature = "async")]
use maybe_async::maybe_async as maybe_async_attr;
#[cfg(not(feature = "async"))]
use maybe_async::must_be_sync as maybe_async_attr;

/// 官方要求至少每 12 小时重新拉取一次平台证书列表。
pub const REFRESH_INTERVAL_SECS: i64 = 12 * 60 * 60;

/// 由**未知 `Wechatpay-Serial`** 触发的刷新，在**成功**之后至少间隔这么多秒。
///
/// 伪造一个 serial 就能触发刷新，而 `GET /v3/certificates` 是官方要求「至少每 12 小时」
/// 的接口 —— 没有这道闸，应答验签本身就成了可被外部触发的流量放大器。
///
/// ⚠ 窗口只在**成功**后计（见 `refresh_platform_keys_for_unknown_serial`）：一次瞬时失败
/// 不该让随后一分钟内所有需要验签的应答都失败。代价是刷新**持续**失败时（例如证书权限
/// 配错），未知 serial 会按请求频率重试证书接口 —— 那种情况下业务本身已经在报错。
pub const UNKNOWN_SERIAL_REFRESH_MIN_INTERVAL_SECS: i64 = 60;

/// 等待并发刷新结束时的轮询间隔（单次等待的上限；总预算见 [`DEFAULT_KEY_REFRESH_WAIT`]）。
const REFRESH_WAIT_INTERVAL: Duration = Duration::from_millis(200);

/// 等待并发平台证书刷新完成的**默认预算**：2 秒（原先硬编码的 200ms × 10 轮的等价值）。
///
/// 刷新是单飞的：同一时刻只有一个请求去打 `/v3/certificates`，其余请求每 200ms 轮询一次
/// 等它结束（见 `wait_for_refresh`）。预算用
/// [`WechatPay::with_key_refresh_wait`](crate::pay::WechatPay::with_key_refresh_wait) 覆盖；
/// 等超预算的请求会拿到 [`PayError::UnknownPlatformSerial`]（消息里带预算毫秒数）——
/// 对自己发起的业务请求而言这是响亮且保守的失败，不会把没验签的应答当成功。
pub const DEFAULT_KEY_REFRESH_WAIT: Duration = Duration::from_millis(2000);

/// 微信支付公钥模式的 `Wechatpay-Serial` 前缀。
///
/// 这种公钥**不在** `GET /v3/certificates` 里（要从商户平台下载自行更新），
/// 所以对它刷新平台证书是白打接口 —— 直接给出可操作的错误。
pub(crate) const PUBLIC_KEY_ID_PREFIX: &str = "PUB_KEY_ID_";

/// 打破 `request` ↔ 证书刷新 这个异步调用环。
///
/// 环的形状：`request` → `verify_response` → `verify_signed` → `refresh_for_unknown_serial`
/// → `fetch_and_install_keys` → `fetch_keys_from_api` → `request`。异步 fn 的 Future 会在
/// 类型层面内联，环会让它**无限大**（编译期 E0733），必须在其中一条边上引入间接。
///
/// 选在「拉证书」这条边上：它每次刷新（12 小时 / 轮换）才走一次，而不是每请求一次 ——
/// 顺带避免在热路径上每请求多一次 `Box::pin` 分配。同步模式下没有这个问题（普通递归调用），
/// 因此这份宏在同步模式下直接求值。
#[cfg(feature = "async")]
macro_rules! indirect {
    ($future:expr) => {
        Box::pin($future).await
    };
}

/// 同步模式：无需间接（见异步版本的说明）。
#[cfg(not(feature = "async"))]
macro_rules! indirect {
    ($future:expr) => {
        $future
    };
}

/// 索引里的一条密钥：PEM 原文 + 首次验签时解析出的公钥。
///
/// 解析结果缓存起来（`Arc` 共享，`Clone` 索引不复制大整数）：应答与回调验签是热路径，
/// 每请求重新解析 PEM 是纯粹的开销 —— 同一把钥匙解析一次就够。
#[derive(Clone)]
struct KeyEntry {
    pem: String,
    parsed: OnceLock<Arc<RsaPublicKey>>,
}

// 手写 `Debug`：`RsaPublicKey` 的 derive 输出会把整个模数打出来，日志里全是噪音。
impl std::fmt::Debug for KeyEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyEntry")
            .field("pem", &self.pem)
            .field("parsed", &self.parsed.get().is_some())
            .finish()
    }
}

impl KeyEntry {
    /// 取已解析的公钥：首次调用时解析并缓存。
    ///
    /// 解析**失败不缓存** —— 坏 PEM 是配置问题，每次都该报同一个
    /// `VerifyError`（消息与 [`util::parse_rsa_public_key`] 一致），而不是把错误也冻住。
    fn parsed_key(&self) -> Result<Arc<RsaPublicKey>, PayError> {
        if let Some(parsed) = self.parsed.get() {
            return Ok(Arc::clone(parsed));
        }
        let parsed = Arc::new(util::parse_rsa_public_key(&self.pem)?);
        Ok(Arc::clone(self.parsed.get_or_init(move || parsed)))
    }
}

/// 平台密钥索引:**两张表** + 轮换表的新鲜度。
///
/// - `injected`:调用方注入的固定密钥(微信支付公钥、自建密钥源),不参与轮换 ——
///   平台证书列表里根本没有它们,整体替换只会把它们抹掉。
/// - `rotating`:自动拉取的平台证书,`GET /v3/certificates` 的安装目标。
///
/// 选键顺序**静态表优先**:同一 serial 两表都有时用注入的那把(官方公钥灰度期,商户自己钉的
/// 公钥是更权威的来源),轮换表那条只是被遮蔽、不会被删除。命中注入表但 PEM / 签名坏时直接失败,
/// **不**回落到轮换表 —— 静默降级会让「密钥配错了」看起来像「轮换还没完成」。
///
/// 不含网络逻辑:由调用方决定何时拉取与刷新,因此可以离线单测、也便于放进
/// 应用状态里跨请求复用。
#[derive(Debug, Default, Clone)]
pub struct PlatformKeys {
    injected: HashMap<String, KeyEntry>,
    rotating: HashMap<String, KeyEntry>,
    fetched_at: Option<i64>,
}

impl PlatformKeys {
    /// 构造一个空索引。
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入或覆盖一把**调用方注入**的密钥(静态表);返回是否覆盖了已有条目。
    ///
    /// 不解析 PEM(失败推迟到首次 [`PlatformKeys::verify`]);覆盖会把旧的解析缓存一并丢弃。
    pub fn insert(
        &mut self,
        serial_no: impl Into<String>,
        public_key_pem: impl Into<String>,
    ) -> bool {
        insert_entry(&mut self.injected, serial_no.into(), public_key_pem.into())
    }

    /// 插入或覆盖一把**轮换表**密钥(平台证书);只给 `GET /v3/certificates` 的安装路径用。
    pub(crate) fn insert_rotating(
        &mut self,
        serial_no: impl Into<String>,
        public_key_pem: impl Into<String>,
    ) -> bool {
        insert_entry(&mut self.rotating, serial_no.into(), public_key_pem.into())
    }

    /// 插入一把**已解析**的注入密钥:配置期的 PEM 当场解析,不推迟到首次验签
    /// (坏 PEM 属于配置错误,应当在注入时就报错)。
    pub(crate) fn insert_parsed(
        &mut self,
        serial_no: String,
        public_key_pem: String,
        parsed: Arc<RsaPublicKey>,
    ) -> bool {
        self.injected
            .insert(
                serial_no,
                KeyEntry {
                    pem: public_key_pem,
                    parsed: OnceLock::from(parsed),
                },
            )
            .is_some()
    }

    /// 只替换**轮换表**(安装一次拉取结果):调用方注入的静态密钥不受影响。
    pub(crate) fn install_rotating(&mut self, fetched: PlatformKeys) {
        self.rotating = fetched.rotating;
    }

    /// 按 `Wechatpay-Serial` 取公钥 PEM(静态表优先,未命中再查轮换表)。
    pub fn get(&self, serial_no: &str) -> Option<&str> {
        self.entry(serial_no).map(|entry| entry.pem.as_str())
    }

    /// 索引里的密钥数量(两表**去重**:同一 serial 两表都有只算一次)。轮换期正常会有 2 个。
    pub fn len(&self) -> usize {
        self.injected.len()
            + self
                .rotating
                .keys()
                .filter(|serial| !self.injected.contains_key(*serial))
                .count()
    }

    /// 索引是否为空(两张表都没有密钥)。
    pub fn is_empty(&self) -> bool {
        self.injected.is_empty() && self.rotating.is_empty()
    }

    /// 当前索引里的全部 serial_no(两表合并去重、排序后),用于排查「到底认了哪几张证书」。
    pub fn serials(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .injected
            .keys()
            .chain(self.rotating.keys())
            .map(String::as_str)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// 记录一次成功拉取的时间(unix 秒)。
    pub fn mark_refreshed(&mut self, now_unix_secs: i64) {
        self.fetched_at = Some(now_unix_secs);
    }

    /// 最近一次成功拉取的时间(unix 秒);从未拉取过则为 `None`。
    pub fn fetched_at(&self) -> Option<i64> {
        self.fetched_at
    }

    /// 轮换表是否该重新拉取了。
    ///
    /// **例外**:调用方已经注入静态密钥、轮换表又从未拉取过(双表并用、且还没遇到需要平台证书的
    /// 场合)时返回 `false` —— 轮换表改为**按需**填充(未知 serial 时由
    /// [`WechatPay::refresh_platform_keys_for_unknown_serial`] 拉一次)。否则纯公钥模式的每个请求
    /// 前都会先拉一次证书接口,而该接口不可达时业务会全盘失败 —— 这正是公钥模式要避免的强依赖。
    pub fn needs_refresh(&self, now_unix_secs: i64) -> bool {
        if self.fetched_at.is_none() && self.rotating.is_empty() && !self.injected.is_empty() {
            return false;
        }
        match self.fetched_at {
            None => true,
            Some(fetched) => now_unix_secs.saturating_sub(fetched) >= REFRESH_INTERVAL_SECS,
        }
    }

    /// 按选键顺序取条目:静态表优先,未命中再看轮换表。
    fn entry(&self, serial_no: &str) -> Option<&KeyEntry> {
        self.injected
            .get(serial_no)
            .or_else(|| self.rotating.get(serial_no))
    }

    /// 用 `serial_no` 对应的密钥验证 `timestamp\nnonce\nbody\n` 的签名。
    ///
    /// 选键顺序与 [`PlatformKeys::get`] 一致(静态表优先);两表都没有该 serial 时返回
    /// [`PayError::UnknownPlatformSerial`] —— 这通常意味着微信正在轮换证书,应当重新拉取列表
    /// 后重试,**不要**退化成用别的密钥去试。
    pub fn verify(
        &self,
        serial_no: &str,
        timestamp: &str,
        nonce: &str,
        body: &str,
        signature: &str,
    ) -> Result<(), PayError> {
        let entry = self
            .entry(serial_no)
            .ok_or_else(|| PayError::UnknownPlatformSerial(serial_no.to_string()))?;
        // 公钥解析结果缓存在条目里:热路径上同一把钥匙只解析一次(失败不缓存)。
        let public_key = entry.parsed_key()?;
        let message = format!("{timestamp}\n{nonce}\n{body}\n");
        util::verify_rsa_sha256_with_key(&public_key, &message, signature)
    }
}

/// 往指定表里插一条未解析的条目;返回是否覆盖了同 serial 的旧条目。
fn insert_entry(
    table: &mut HashMap<String, KeyEntry>,
    serial_no: String,
    public_key_pem: String,
) -> bool {
    table
        .insert(
            serial_no,
            KeyEntry {
                pem: public_key_pem,
                parsed: OnceLock::new(),
            },
        )
        .is_some()
}

impl WechatPay {
    /// 拉取 `GET /v3/certificates`，解密每一张平台证书并建立 `serial_no -> 公钥 PEM` 索引。
    ///
    /// 返回值是**全新**的索引：轮换期微信会同时下发新旧两张，因此调用方应当整体替换
    /// 手里的旧索引，而不是逐条合并（合并会把微信已经撤下的旧证书永久留在索引里）。
    /// 已过期的证书会被丢弃。
    ///
    /// ⚠ 这是**无状态**的：只把索引返回给调用方，不写进客户端 —— 客户端内部那份由
    /// [`WechatPay::refresh_platform_keys`] 维护，出站应答验签用的是它。
    ///
    /// 拉取成功后请调用 [`PlatformKeys::mark_refreshed`] 记录时间，以便
    /// [`PlatformKeys::needs_refresh`] 判断下次刷新时机。
    #[maybe_async_attr]
    pub async fn fetch_platform_keys(&self) -> Result<PlatformKeys, PayError> {
        self.fetch_keys_from_api().await
    }

    /// 拉取平台证书，并把新索引**整体替换**进客户端（出站应答验签用的就是它）。
    ///
    /// **单飞**：同一时刻只有一个人打证书接口，其余调用等它结束后用它的结果。
    /// 刷新失败不会动已有索引：宁可继续用旧证书，也不要变成「没有密钥可用」。
    ///
    /// 静态密钥模式（[`WechatPay::with_platform_public_key`] /
    /// [`WechatPay::set_platform_keys`]）下返回错误：平台证书列表里没有你配置的那些
    /// key，整体替换只会把它们抹掉。
    ///
    /// ⚠ 触发源是**未鉴权输入**时（回调里的 `Wechatpay-Serial`）请改用
    /// [`WechatPay::refresh_platform_keys_for_unknown_serial`] —— 它多一道 60s 限流。
    #[maybe_async_attr]
    pub async fn refresh_platform_keys(&self) -> Result<PlatformKeys, PayError> {
        if !self.auto_refresh_keys() {
            return Err(PayError::VerifyError(
                "当前客户端使用调用方提供的密钥（公钥模式 / set_platform_keys），不会拉取平台证书；\
                 要更新请重新调用 set_platform_keys 或重建客户端"
                    .to_string(),
            ));
        }
        self.fetch_and_install_keys().await
    }

    /// 遇到未知 `Wechatpay-Serial` 时的刷新入口：**60s 最小间隔 + 单飞**。
    ///
    /// 出站应答的验签由 SDK 内部走这条路；**回调验签**拿到
    /// [`PayError::UnknownPlatformSerial`] 时也应当调它，而不是
    /// [`WechatPay::refresh_platform_keys`] —— 回调里的 serial 是未鉴权输入，
    /// 伪造一个就能触发刷新，没有限流等于把 `/v3/certificates`（官方要求 12 小时一次的
    /// 接口）变成可被外部触发的放大器。
    ///
    /// 返回 `Ok` 只表示刷新流程结束（也可能由并发的另一次刷新完成）：调用方应重新取
    /// [`WechatPay::platform_keys`] 快照再验一次，仍未知就按「轮换尚未完成」处理。
    #[maybe_async_attr]
    pub async fn refresh_platform_keys_for_unknown_serial(
        &self,
        serial: &str,
    ) -> Result<(), PayError> {
        // `PUB_KEY_ID_…`（微信支付公钥）**不在**平台证书列表里：刷新是白打接口，而且回调里的
        // serial 来自未鉴权输入 —— 放到限流之前拦掉，伪造的公钥 ID 连一次证书请求都触发不了。
        // 静态表里没有它只有两种可能：公钥还没注入，或微信换了公钥（要重新下载再注入）。
        if serial.starts_with(PUBLIC_KEY_ID_PREFIX) {
            return Err(PayError::UnknownPlatformSerial(format!(
                "{serial}:该 serial 是微信支付公钥（不在平台证书列表里），拉取证书无法认识它；\
                 请用 add_static_platform_key（或 with_platform_public_key）注入这把公钥后重试"
            )));
        }
        if !self.auto_refresh_keys() {
            return Err(PayError::UnknownPlatformSerial(format!(
                "{serial}:当前客户端使用调用方提供的密钥（公钥模式 / set_platform_keys），\
                 不会自动拉取平台证书"
            )));
        }
        // ⚠ 顺序：**先抢单飞标记**，再判断限流。反过来的话，等待者可能「读 `in_flight`
        // 时还没人刷、判断限流时却读到了别人刚写入的时间戳」，被误判成「刚刷新过」而拿到
        // 一个虚假的 `UnknownPlatformSerial`（窄窗口，但会让并发用例偶发变红）。
        let Some(_guard) = RefreshGuard::acquire(&self.key_refresh) else {
            // 已经有人在刷：等它结束（它成功的话索引里就有这个 serial 了），不做限流判断 ——
            // 等同伴不算「刚刷新过」。
            self.wait_for_refresh().await?;
            return Ok(());
        };
        let now = crate::util::now_unix_secs();
        let last_success = self
            .key_refresh
            .last_unknown_serial_refresh
            .load(Ordering::SeqCst);
        if last_success != 0
            && now.saturating_sub(last_success) < UNKNOWN_SERIAL_REFRESH_MIN_INTERVAL_SECS
        {
            return Err(PayError::UnknownPlatformSerial(format!(
                "{serial}:距上次刷新平台证书不足 {UNKNOWN_SERIAL_REFRESH_MIN_INTERVAL_SECS}s，\
                 已跳过重复刷新（等微信的证书更新后再试）"
            )));
        }
        let result = self.fetch_and_install_keys_once().await;
        // ⚠ 只在**成功**后计窗口：一次瞬时失败（网络抖动、上游 5xx）不该让随后一分钟内所有
        // 需要验签的应答都失败 —— 下一批请求应当能立刻再试。代价是刷新**持续**失败时，
        // 未知 serial 会按请求频率重试证书接口；那种情况下业务本身已经在报错，可观测性足够，
        // 而「一次失败 = 一分钟全挂」的代价更常见也更难受。
        if result.is_ok() {
            self.key_refresh
                .last_unknown_serial_refresh
                .store(now, Ordering::SeqCst);
        }
        // ⚠ 刷新失败必须归一到 `UnknownPlatformSerial`（R7）：这个错误来自**另一条请求**
        // （拉证书）。若让它以原类型逃进业务请求的失败分类，会同时踩两个坑：
        // 连接类 / 429 / 5xx 会被判成「可重试」→ **重发那条应答已经收到的请求**
        // （R4 明确要求绝不重发业务请求）；4xx 又会让调用方拿到
        // `may_have_taken_effect() == false`（「确定没受理」），可微信明明已经回过它 ——
        // 正是这条无法验签的应答触发了本次刷新。
        result.map(|_| ()).map_err(|err| match err {
            // 等待并发刷新超时这类错误已经是同一个结论，别套娃。
            PayError::UnknownPlatformSerial(message) => PayError::UnknownPlatformSerial(message),
            other => PayError::UnknownPlatformSerial(format!(
                "{serial}:应答由未知 serial 签名，刷新平台证书也失败了（{other}），无法完成验签"
            )),
        })
    }

    /// 确保密钥索引可用：轮换表按 12 小时窗口刷新（索引为空时就是冷启动引导）。
    ///
    /// 由 `request_json` / `request_no_content` 在**发送前**调用。静态密钥模式与关闭验签时
    /// 什么都不做 —— 密钥要么由调用方负责，要么根本用不上。
    ///
    /// 刷新失败**不必然**让请求失败：只要索引里还有一份可用密钥（典型场景是调用方注入的
    /// 静态公钥 + 轮换表刷新失败），请求照发 —— 那份密钥能验的应答/回调仍然验得过，
    /// 验不了的会按「未知 serial」单独失败。只有一份密钥都没有时才把拉取错误原样带出。
    #[maybe_async_attr]
    pub(crate) async fn ensure_keys(&self) -> Result<(), PayError> {
        if self.response_verify == ResponseVerify::Disabled || !self.auto_refresh_keys() {
            return Ok(());
        }
        if !self.keys_need_refresh() {
            return Ok(());
        }
        let refreshed = self.refresh_platform_keys().await;
        if self.platform_keys_read().is_empty() {
            refreshed?;
            return Err(PayError::VerifyError(
                "平台证书索引为空：自动拉取平台证书失败或仍在进行中，无法验证应答签名".to_string(),
            ));
        }
        // ⚠ 绑定写成 `_err` 不是笔误：这里的失败只进 `debug!`，而 `debug!` 在未启用
        // `debug-print` 时展开为空操作、参数不求值（`src/macros.rs`）。用 `err` 会让
        // 默认构建在 CI 的 `-D warnings` 下因 `unused_variables` 直接编译失败。
        if let Err(_err) = refreshed {
            debug!("平台证书刷新失败，继续用现有索引（静态密钥仍可验签）: {_err}");
        }
        Ok(())
    }

    /// 当前索引是否需要刷新（读锁只在语句内存活，不跨 `.await`）。
    fn keys_need_refresh(&self) -> bool {
        self.platform_keys_read()
            .needs_refresh(crate::util::now_unix_secs())
    }

    /// 拉取 → 自校验 → 整体替换 + 记录刷新时间。失败时不动已有索引。
    ///
    /// **单飞**：同一时刻只有一个人打 `/v3/certificates`；CAS 抢不到标记的人等它结束，
    /// 然后用手里的索引（调用方会重新查表验签，仍未知就按「轮换未完成」处理）。
    /// 冷启动与 12 小时窗口的并发请求同样走这里 —— 否则 N 个并发请求会对官方要求
    /// 「至少每 12 小时一次」的接口发出 N 次请求。
    #[maybe_async_attr]
    async fn fetch_and_install_keys(&self) -> Result<PlatformKeys, PayError> {
        // ⚠ 抢不到标记时只**等**，不要用 `needs_refresh` 当完成信号：命中未知 serial 时索引
        // 可能既新鲜又不含那个 serial（正轮到别人去装），用它判断会让等待者抢在安装之前
        // 返回旧索引。
        if let Some(_guard) = RefreshGuard::acquire(&self.key_refresh) {
            return self.fetch_and_install_keys_once().await;
        }
        self.wait_for_refresh().await?;
        Ok(self.platform_keys())
    }

    /// 有界等待正在进行的刷新结束：轮询 `in_flight`，预算由
    /// [`WechatPay::with_key_refresh_wait`](crate::pay::WechatPay::with_key_refresh_wait) 决定。
    #[maybe_async_attr]
    async fn wait_for_refresh(&self) -> Result<(), PayError> {
        let budget = self.key_refresh_wait;
        let started = Instant::now();
        loop {
            if !self.key_refresh.in_flight.load(Ordering::SeqCst) {
                return Ok(());
            }
            // 预算耗尽即收手（预算为 `Duration::ZERO` 时在这里立即命中 —— 等价于「不等待」）。
            let elapsed = started.elapsed();
            if elapsed >= budget {
                break;
            }
            // 单次等待不超过剩余预算：短预算下不会睡满一整轮。
            sleep_for(REFRESH_WAIT_INTERVAL.min(budget - elapsed)).await;
        }
        // 睡完最后一轮再确认一次，避免刚好卡在边界上。
        if !self.key_refresh.in_flight.load(Ordering::SeqCst) {
            return Ok(());
        }
        Err(PayError::UnknownPlatformSerial(format!(
            "平台证书正在刷新中，等待超过 {}ms 仍未完成",
            budget.as_millis()
        )))
    }

    /// 真正去拉一次并安装（调用方已持有单飞标记）。
    #[maybe_async_attr]
    async fn fetch_and_install_keys_once(&self) -> Result<PlatformKeys, PayError> {
        // `indirect!` 是为了打破 `request ↔ 证书刷新` 的异步调用环（见宏的说明）。
        let keys = indirect!(self.fetch_keys_from_api())?;
        {
            let mut guard = self.platform_keys_write();
            // 装之前**复查**：这期间调用方可能已经 `set_platform_keys` /
            // `with_platform_public_key` 切到静态密钥模式 —— 那时覆盖掉他灌进来的密钥是
            // 不可逆的（静态模式下不会再有下一次自动拉取）。`static_keys` 只挡「发起」，
            // 挡不住「安装」，所以这道复查必须在写锁内。
            if !self.auto_refresh_keys() {
                debug!("客户端已切到静态密钥模式，放弃安装本次拉取结果");
                return Ok(guard.clone());
            }
            // 只替换轮换表:调用方注入的静态公钥不属于这次拉取的结果
            guard.install_rotating(keys.clone());
            guard.mark_refreshed(crate::util::now_unix_secs());
        }
        Ok(keys)
    }

    /// 拉取证书列表、解密建索引并自校验（不落库）。
    #[maybe_async_attr]
    async fn fetch_keys_from_api(&self) -> Result<PlatformKeys, PayError> {
        // 证书列表的应答**不能**先按索引验签（轮换期它由本地还不认识的新证书签名），
        // 所以这条请求走自校验模式：拿到响应里的证书后再回头验它。
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
        let keys = self.build_keys_from_response(&response)?;

        if keys.is_empty() {
            return Err(PayError::VerifyError(
                "平台证书列表为空（或全部已过期），无法建立密钥索引".to_string(),
            ));
        }
        if self.response_verify == ResponseVerify::Required {
            self.self_check_certificate_response(&raw.headers, &raw.body, &keys)?;
        }
        Ok(keys)
    }

    /// 把证书列表响应解密成 `serial_no -> 公钥 PEM` 索引，丢弃已过期的证书。
    ///
    /// 不校验「是否为空」：空列表对 [`WechatPay::certificates`] 是合法结果（公钥模式的商户
    /// 可能压根没有平台证书），只有需要拿它验签的调用方才该把空列表当成失败。
    pub(crate) fn build_keys_from_response(
        &self,
        response: &CertificateResponse,
    ) -> Result<PlatformKeys, PayError> {
        let mut keys = PlatformKeys::new();
        for cert in response.data.iter().flatten() {
            let Certificate {
                serial_no,
                encrypt_certificate,
                ..
            } = cert;
            let EncryptCertificate {
                ciphertext,
                nonce,
                associated_data,
                ..
            } = encrypt_certificate;

            let plaintext = self.decrypt_bytes(ciphertext, nonce, associated_data)?;
            // 过期证书直接丢弃：留着它只会让「拿别的密钥去试」看起来可行。
            // （`x509_is_valid` 的第二个返回值是 not_after，需要精确时间时用它。）
            let (valid, _) = util::x509_is_valid(plaintext.as_slice())?;
            if !valid {
                debug!("丢弃已过期的平台证书 {serial_no}");
                continue;
            }
            let public_key_pem = util::x509_to_pem(plaintext.as_slice())
                .map_err(|e| PayError::VerifyError(format!("平台证书解析失败: {e}")))?;
            // 拉来的证书进**轮换表**:调用方注入的静态密钥不能被平台证书列表覆盖
            keys.insert_rotating(serial_no.clone(), public_key_pem);
        }
        Ok(keys)
    }

    /// 用**本次响应下发**的、`Wechatpay-Serial` 指向的那张证书验证本次响应。
    ///
    /// ⚠ 这不是信任锚，也不是安全边界：公钥就来自同一个（尚未验签的）body，
    /// 真正的保证是 TLS 与「只有商户和微信持有 `v3_key`」的 AEAD 解密。
    /// 它成立的意义在于**可用性**：轮换期微信用新证书签名，本地索引里还没有新证书 ——
    /// 没有这条自证路径就永远学不到新证书（刷新请求自己验不过，见 R4/R5）。
    pub(crate) fn self_check_certificate_response(
        &self,
        headers: &HeaderMap,
        body: &[u8],
        keys: &PlatformKeys,
    ) -> Result<(), PayError> {
        let headers = crate::notify::response_signature_headers(headers).map_err(|missing| {
            PayError::VerifyError(format!(
                "平台证书应答缺少签名头 {}，无法自校验；若链路上有代理 / CDN，\
                 请检查它们是否过滤了 Wechatpay-* 头",
                missing.join(" / ")
            ))
        })?;
        crate::notify::check_timestamp_skew(&headers.timestamp, crate::util::now_unix_secs())?;
        let body = std::str::from_utf8(body).map_err(|e| {
            PayError::VerifyError(format!(
                "平台证书应答不是合法 UTF-8，无法按原始字节验签: {e}"
            ))
        })?;
        keys.verify(
            &headers.serial,
            &headers.timestamp,
            &headers.nonce,
            body,
            &headers.signature,
        )
        .map_err(|err| match err {
            PayError::UnknownPlatformSerial(serial) => PayError::UnknownPlatformSerial(format!(
                "{serial}:证书列表应答由该 serial 签名，但响应体里没有这张证书，无法自校验"
            )),
            other => other,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 `tests/offline.rs` 的测试私钥（PKCS#8）配套的公钥。
    ///
    /// 公开信息，只为覆盖索引的「解析一次、验签多次」路径；与离线用例用的是同一对密钥。
    const TEST_PUBLIC_KEY_PEM: &str = "\
-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAxu8a4jjKvwg7moQK4AIy
bEBk/IASqR1l65QmxZCQ+uaC7lE1cpbhtW0S8oVs6oe1jrkKiE5lFfrNQzzfJbmo
jW1X5Owu3tEpb3LhH8guROBP8QPe4qQ547VczF01nkTIm86m0969XZIM5NOiRrCo
KyUKtJqnNFH1C1FSfm/oqBOMI2AO1Gz0JqJI/yQJOYqMt3wh/kJ3w7LizL2NBlgr
V3P7rln5q8tfDz9Xzlf/5zfPkxkbUxYcfhTSwrypplKuaSBpSvzMubZH/EcnBr/D
tiijCX4njiSVZ2j4cZIJoKpdajYDFRxmn6Ko3oRT0zzVmHdioqNkbgn/mMf2w/1R
awIDAQAB
-----END PUBLIC KEY-----
";

    #[test]
    fn verify_parses_each_public_key_only_once() {
        let mut keys = PlatformKeys::new();
        assert!(!keys.insert("SERIAL_CACHED", TEST_PUBLIC_KEY_PEM));
        assert_eq!(keys.get("SERIAL_CACHED"), Some(TEST_PUBLIC_KEY_PEM));

        // 签名本身无效（不是这把钥匙签的）没关系：走的就是「查表 → 取公钥 → 验签」。
        // 计数是**线程局部**的（`util::RSA_PEM_PARSE_COUNT`），断言用的又是本用例内的增量，
        // 因此不需要与其它用例互斥。
        let before = util::rsa_pem_parse_count();
        let first = keys.verify("SERIAL_CACHED", "1700000000", "nonce", "{}", "AAAA");
        assert!(first.is_err(), "无效签名必须报错: {first:?}");
        assert_eq!(
            util::rsa_pem_parse_count(),
            before + 1,
            "首次验签应解析一次公钥"
        );
        assert!(
            keys.injected["SERIAL_CACHED"].parsed.get().is_some(),
            "首次验签后应把解析结果缓存进条目"
        );

        let second = keys.verify("SERIAL_CACHED", "1700000000", "nonce", "{}", "AAAA");
        assert_eq!(
            util::rsa_pem_parse_count(),
            before + 1,
            "第二次验签不得再解析 PEM"
        );
        match second {
            // 走到了 RSA 验签本身（缓存命中的证据），而不是又去解析 PEM。
            Err(PayError::VerifyError(message)) => assert!(
                !message.contains("parser error"),
                "第二次不该重新解析 PEM: {message}"
            ),
            other => panic!("应为 VerifyError，实际 {other:?}"),
        }

        // 未知 serial 与坏 PEM 的行为保持原样（错误类型与文案都不变）。
        match keys.verify("SERIAL_MISSING", "1700000000", "nonce", "{}", "AAAA") {
            Err(PayError::UnknownPlatformSerial(serial)) => assert_eq!(serial, "SERIAL_MISSING"),
            other => panic!("应为 UnknownPlatformSerial，实际 {other:?}"),
        }
        keys.insert("SERIAL_BAD", "not a pem");
        match keys.verify("SERIAL_BAD", "1700000000", "nonce", "{}", "AAAA") {
            Err(PayError::VerifyError(message)) => {
                assert!(
                    message.contains("public key parser error"),
                    "实际: {message}"
                );
            }
            other => panic!("应为 VerifyError，实际 {other:?}"),
        }
    }

    /// 两表的归属、选键顺序（静态优先）与「安装拉取结果只动轮换表」
    #[test]
    fn two_tables_keep_their_own_entries() {
        let mut keys = PlatformKeys::new();
        assert!(!keys.insert("SERIAL_INJECTED", TEST_PUBLIC_KEY_PEM));
        assert!(!keys.insert_rotating("SERIAL_ROTATING", TEST_PUBLIC_KEY_PEM));
        assert_eq!(keys.serials(), vec!["SERIAL_INJECTED", "SERIAL_ROTATING"]);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys.get("SERIAL_ROTATING"), Some(TEST_PUBLIC_KEY_PEM));

        // 同一 serial 两表都有：静态表获胜，且只算一个 serial
        keys.insert("SERIAL_BOTH", "INJECTED_PEM");
        keys.insert_rotating("SERIAL_BOTH", "ROTATING_PEM");
        assert_eq!(keys.get("SERIAL_BOTH"), Some("INJECTED_PEM"));
        assert_eq!(
            keys.serials(),
            vec!["SERIAL_BOTH", "SERIAL_INJECTED", "SERIAL_ROTATING"]
        );
        assert_eq!(keys.len(), 3, "同一 serial 两表都有只算一次");
        // 注入表那条坏 PEM：不许回落到轮换表（静默降级会让配错密钥看起来像「轮换还没完成」）
        assert!(matches!(
            keys.verify("SERIAL_BOTH", "1700000000", "nonce", "{}", "AAAA"),
            Err(PayError::VerifyError(_))
        ));

        // 安装一次拉取结果：只替换轮换表，注入的密钥必须留下
        let mut fetched = PlatformKeys::new();
        fetched.insert_rotating("SERIAL_FRESH", TEST_PUBLIC_KEY_PEM);
        keys.install_rotating(fetched);
        assert_eq!(keys.get("SERIAL_INJECTED"), Some(TEST_PUBLIC_KEY_PEM));
        assert_eq!(keys.get("SERIAL_ROTATING"), None, "轮换表是整体替换");
        assert_eq!(keys.get("SERIAL_FRESH"), Some(TEST_PUBLIC_KEY_PEM));
    }

    /// `needs_refresh` 的冷启动例外：**已注入静态密钥且从未拉取过**时返回 false
    /// （轮换表改为按需拉取），其余情况保持原有时间窗语义。
    #[test]
    fn needs_refresh_skips_the_cold_start_only_when_a_static_key_exists() {
        let now = 1_700_000_000;

        // 两张表都空：冷启动必须拉
        assert!(PlatformKeys::new().needs_refresh(now));

        // 只有注入的静态密钥、从未拉取：不预拉
        let mut injected_only = PlatformKeys::new();
        injected_only.insert("SERIAL_INJECTED", TEST_PUBLIC_KEY_PEM);
        assert!(!injected_only.needs_refresh(now));

        // 轮换表有内容且已过窗口：照刷
        let mut stale = injected_only.clone();
        stale.insert_rotating("SERIAL_ROT", TEST_PUBLIC_KEY_PEM);
        stale.mark_refreshed(now - REFRESH_INTERVAL_SECS - 1);
        assert!(stale.needs_refresh(now));

        // 轮换表新鲜：不刷
        let mut fresh = stale.clone();
        fresh.mark_refreshed(now - 10);
        assert!(!fresh.needs_refresh(now));

        // 只有轮换表、且从未标记过拉取时间：要刷
        let mut rotating_only = PlatformKeys::new();
        rotating_only.insert_rotating("SERIAL_ROT", TEST_PUBLIC_KEY_PEM);
        assert!(rotating_only.needs_refresh(now));
    }

    /// 造一个「已注入静态公钥 + 轮换表已过刷新窗口 + 刷新必然失败」的客户端：
    /// 占位私钥在**签名阶段**就报 `SignError`（请求根本不会发出），而索引里还剩一份能验签的密钥。
    fn client_with_static_key_and_a_stale_rotating_table() -> WechatPay {
        let client = WechatPay::from_config(crate::pay::WechatPayConfig {
            appid: "wx_test_appid".to_string(),
            mch_id: "1900000001".to_string(),
            private_key: "not-parsed-until-first-sign".to_string(),
            serial_no: "5F3A1B7C9D2E4F6081A2B3C4D5E6F708192A3B4C".to_string(),
            v3_key: "0123456789abcdef0123456789abcdef".to_string(),
            notify_url: "https://example.com/notify".to_string(),
            response_verify: ResponseVerify::Required,
        })
        // 私钥是占位串 ⇒ 签名必失败；地址仍钉在死端口，万一私钥被换成合法 fixture 也不会出网。
        // 关掉重试免得等退避。
        .with_base_url("http://127.0.0.1:1")
        .with_retry(crate::retry::RetryPolicy::disabled());
        client
            .add_static_platform_key("PUB_KEY_ID_INLINE_TEST", TEST_PUBLIC_KEY_PEM)
            .expect("注入公钥");

        // 造「轮换表已过刷新窗口」的状态；注入的静态密钥仍在
        {
            let mut guard = client.platform_keys_write();
            guard.insert_rotating("SERIAL_ROTATING_STALE", TEST_PUBLIC_KEY_PEM);
            guard.mark_refreshed(crate::util::now_unix_secs() - REFRESH_INTERVAL_SECS - 1);
        }
        assert!(client.keys_need_refresh(), "前置状态：轮换表已过刷新窗口");
        client
    }

    /// 双表并存时轮换表刷新失败**不该**让请求失败：注入的静态密钥仍能验签。
    ///
    /// 回归点：`ensure_keys` 原先无条件 `?` 传播拉取错误 —— 公钥模式加上轮换表之后，
    /// 证书接口一抖动就会把「本来验得过的公钥签名报文」一起打掉。
    ///
    /// ⚠ 两种模式各覆盖一遍：`src/async_impl/` 只是共享实现的栖身处，默认（同步）构建
    /// 同样编译 `ensure_keys`，只留 async 版等于把同步路径的回归留给调用方。
    #[cfg(feature = "async")]
    #[tokio::test]
    async fn refresh_failure_does_not_fail_requests_when_a_static_key_remains() {
        client_with_static_key_and_a_stale_rotating_table()
            .ensure_keys()
            .await
            .expect("有静态密钥时，轮换表刷新失败不该让请求失败");
    }

    /// 同上，同步构建（这里 `ensure_keys` 是普通函数，不是 `async fn`）。
    #[cfg(not(feature = "async"))]
    #[test]
    fn refresh_failure_does_not_fail_requests_when_a_static_key_remains() {
        client_with_static_key_and_a_stale_rotating_table()
            .ensure_keys()
            .expect("有静态密钥时，轮换表刷新失败不该让请求失败");
    }
}
