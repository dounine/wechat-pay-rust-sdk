# Changelog

记录本仓库的**破坏性变更与行为变更**。格式参考
[Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循
[语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

> 相对 crates.io 上的 `0.2.21`，本节的改动含破坏性变更，`Cargo.toml` 的 `version` 已按 0.x
> 语义同步为 `0.3.0`；发版时把本节标题改名为 `## [0.3.0] - <日期>` 即可。

### 破坏性变更

- **`WechatPay::new(...)` → `WechatPay::from_config(WechatPayConfig { … })`**：六个位置参数
  （`appid` / `mch_id` / `private_key` / `serial_no` / `v3_key` / `notify_url`）都是同一种字符串，
  写反了照样编译，要等到网关 401 才暴露。现在用具名字段构造。
- **`WechatPayTrait` 的访问器改为返回 `&str`**（原先各返回一份 `String` 拷贝：
  `v3_key` 每次解密、`base_url` 每次重试都会多拷一份）。
- **签名接口改为可失败**：`rsa_sign` / `mut_sign_data` 返回 `Result<_, PayError>`；
  `mut_sign_data` 的前缀参数由 `&str` 改为 `PackagePrefix` 枚举（`Bare` / `PrepayId`）——
  两个同类型字符串参数写反了只会在前端拉起支付时才失败。商户私钥 PEM 解析失败现在是
  `PayError::SignError`，**不再 panic**（解析结果只在首次签名时做，之后缓存）。
- **`get_weixin()` 返回 `Result<String, PayError>`**（原先的 `Ok(None)` 不可能出现），
  并且现在会检查 HTTP 状态码（过期 `h5_url` / CDN 错误页不再被当成支付页扫描）、
  Referer 含非法字符时返回错误而不是 panic。
- **`PayError::WechatError(String)` 已删除**（crate 内从无构造点，却逼着下游写一个永远
  不会命中的 match 分支）；新增 `PayError::SignError(String)`（本地签名失败：私钥 PEM 解析、
  RSA 签名运算，或签名请求头构造失败）。
- **五个下单响应类型删除恒为 `None` 的 `code` / `message`**：2xx 错误信封在传输层就归一成
  `Err`，成功路径下这两个字段不可能有值，留着只会诱导调用方写死代码。
- **`WechatPayDecodeData::attach` 改为 `Option<String>`**：官方在下单未传 `attach` 时不会返回
  该字段，原先必填会让**真实支付通知**解密后解析失败。
- **`OrderDetail::goods_detail` 改为 `Option<Vec<GoodsDetail>>`**：官方取值区间是 1..6000，
  原先没有「不传」的表达方式，只能发出越界的空数组。
- **`ParamsTrait` 现在以 `Serialize` 为 supertrait，`to_json` 返回 `Result`**：原先每个实现里各写一份
  `serde_json::to_string(self).unwrap()`（库路径上的 panic），现在由 trait 提供默认实现（7 个实现体
  因此变成空 `impl`）。⚠ 自定义请求参数类型需要自己实现 `serde::Serialize`。
- **`H5Type` 的 `Serialize` 与 `Display` 统一**：序列化改为官方取值 `iOS`（原先输出 `Ios`）。
- 移除 `error!` 宏与 `chrono` 依赖；内部 `debug!` 宏不再 `#[macro_export]` 到 crate 根。
- **`WechatPayConfig` 新增必填字段 `response_verify`**：用具名字面量构造配置的地方都要补
  `response_verify: ResponseVerify::Required,`（不留 `Default` 是有意的 —— 这是一个需要显式
  做出的安全选择，漏写就是编译错误而不是静默放行）。
- **默认强制应答验签**：对**指向本地 mock 网关的测试**是破坏性的 —— 未签名的应答会被
  `VerifyError` 拒绝。迁移方式：让测试网关也签名（参考 `tests/offline.rs` 的 mock），
  或显式 `with_response_verify(ResponseVerify::Disabled)`。
- **微信支付公钥模式的商户必须配置公钥**（`with_platform_public_key`），否则所有应答都会以
  `UnknownPlatformSerial` 失败。
- **`PayError::ApiError` 新增 `request_id: Option<String>`**（应答头 `Request-ID`，缺失为 `None`）：
  对 `ApiError` 做字段穷尽匹配的代码需要补 `..`。`Display` 在取到该值时附 ` (request-id: …)`，
  取不到时与原先逐字节一致。
- **`refunds()` 返回类型变更**：`Result<WeChatResponse<RefundsResponse>, PayError>` →
  `Result<RefundsResponse, PayError>`。失败不再以「带内错误」形式返回。
- **删除 `WeChatResponse<T>`**：在新契约下它的 `Err` 变体永不可达，留着只会让人以为仍需手工
  检查 `is_success()`；`is_success()` / `ok()` / `err()` 一并移除。
- **`WechatPay` 字段私有化**：原先全部是 `pub`（`private_key` / `v3_key` / `base_url` 都不该被
  外部随手改写）。要覆盖网关地址请用 `with_base_url`。
- **`PayError` 新增变体**：`ApiError`、`UnknownPlatformSerial`、`StaleNotify`。
  对 `PayError` 做穷尽 `match` 的代码需要补分支（用 `kind()` 可做粗分类）。
- **`ErrorResponse` 新增 `detail` 字段**：以结构体字面量构造它的代码需要补该字段。
- **`cargo test --doc` 不再是受支持的命令**：README 作为 crate 文档，但其中示例需要真实密钥与
  公网，已设 `[lib] doctest = false`。

### 行为变更

- **`PlatformKeys::needs_refresh` 在「调用方已注入静态密钥、轮换表又从未拉取过」时返回 `false`**：
  轮换表改为**按需**填充（未知 serial 时拉一次）。否则纯公钥模式（双表并用）的每个请求前都会
  先打一次 `/v3/certificates`，而该接口不可达时业务会全盘失败 —— 那正是公钥模式要避免的强依赖。
  轮换表一旦有内容，12 小时窗口语义不变。
- **`ensure_keys`（请求前的密钥自检）不再无条件传播拉取失败**：只要索引里**还有一份可用密钥**
  （典型是注入的静态公钥 + 该次拉取失败），请求照发，失败只记 `debug` 日志；一份密钥都没有时
  仍会返回 `VerifyError` 或原拉取错误。验签本身不放松：验不过的应答仍按未知 serial 单独失败。
- **未知 `Wechatpay-Serial` 的 `PUB_KEY_ID_…` 分支从应答验签移至刷新入口**
  （`refresh_platform_keys_for_unknown_serial`）：语义与文案不变（仍含
  `with_platform_public_key` 的指引，并补上 `add_static_platform_key`），效果是**回调路径**同样
  不再为伪造的公钥 ID 打一次 `/v3/certificates`。
- **`refresh_platform_keys` / 证书安装现在只替换轮换表**（不再 `*guard = keys` 整体覆盖）：
  注入的静态密钥不会被一次平台证书拉取抹掉。`set_platform_keys` 仍是**整体替换两张表**并切
  静态模式（调用方提供的就是全部可信材料）。
- **`SignData` 的 JSON 键名改为官方大小写**：`timestamp` → `timeStamp`、`nonce_str` → `nonceStr`、
  `sign_type` → `signType`、`pay_sign` → `paySign`、`app_id` → `appId`（Rust 字段名不变）。
  此前序列化出来全是 snake_case，而 `wx.requestPayment` 的参数名是官方定死的 camelCase
  （`timeStamp` 的 `S` 还必须大写）—— 直接把 `sign_data` 交给前端会**拉起失败**，且前端只会报
  「缺少参数」，不会指向 SDK。⚠ 若你在前端或用例里手写过这些键名，需要同步改。
  另注意：**APP 支付的拉起参数名与 JSAPI 完全不同**（`appid` / `partnerid` / `prepayid` /
  `package` / `noncestr` / `timestamp` / `sign`），不能复用这里的键名，需自行映射。
- **新增的重试只覆盖三类有官方依据的失败**：确定没送到、微信明确未受理、202。
  **写接口超时（结果未知）不重试** —— 官方口径是先用查单接口确认状态；只读接口
  （查单 / 退款查询 / 平台证书）超时照常重试，因为重放纯读没有副作用。
- **429 / 500 / 502 / 503 现在会自动重试**，这会改变既有错误路径的可观察结果：
  多次尝试后返回的是**最后一次**尝试的错误体（原先只发一次、返回第一次的响应体）。
  既有测试中依赖「单个响应体」的用例已显式关掉重试以隔离关注点。
- **关单不再把「HTTP 200 + 错误信封」当成成功**。走 `request_no_content` 的路径原先根本
  不看响应体，微信以 200 返回 `{"code":"RULE_LIMIT",...}` 时会静默返回 `Ok(())` ——
  尽管 README 一直承诺「包括 HTTP 200 但 body 是错误信封，都返回 Err」。现在错误信封在
  传输层就被归一成 `Err`，与非 2xx 走同一条判定。
- **202 的去向变了**：以前被当成 2xx 成功放行，再拿空 body 去解析，报出一个与真实原因
  无关的 JSON 错误；现在按官方要求重发，重试用尽后降级成带状态码 202 的 `ApiError`。
  注意 202 意味着**请求已被接收、可能随后生效**，所以 `may_have_taken_effect()` 对它返回
  `true` —— 拿到它应当去查单，而不是换个单号重开。
- 每次重试都**重新签名**（新 nonce + 新 timestamp）：重试可能跨过 5 分钟的签名有效窗口，
  复用旧签名会直接 401。
- `async` feature 新增对 `tokio`（仅 `time`）的依赖，用于退避的异步 sleep。
- **无签名头的 4xx 现在返回 `VerifyError`（`Local`）而不是 `ApiError`** —— 这是有意的：
  这类应答的 `code` 会被当成业务结论，不能让它来自无法验证的来源。代价是**拿不到**
  `response.code`：`ORDER_NOT_EXIST` 这条处置路径在验签失败时**不适用**，必须按「结果未知」
  处置（查单 + 告警），不得当成「订单不存在」。
- **无签名头的 5xx 仍是 `ApiError`，但消息带 `[未验签]` 标记**：5xx 不含可被对手用来误判业务
  的语义（伪造它只能诱发重试），而严格拒绝会把 CDN / 网关的 5xx 从「自动重试」变成「不重试」
  —— 正好抹掉本仓的重试设计。`code` / `detail` 保留，`SYSTEM_ERROR` 照常重试。
- **`may_have_taken_effect()` 对验签类错误由 `false` 变为 `true`**（`VerifyError` /
  `UnknownPlatformSerial` / `StaleNotify` 在 `classify()` 里归 `Delivery::Processed`）。
  理由：应答是**完整收到之后**才验签的，请求早已送达微信；官方还会故意下发带
  `WECHATPAY/SIGNTEST/` 前缀的错误签名探测验签实现。判 `false`（= 确定没发生）会诱导写接口
  换个单号重开，正是本 crate 一直在避免的方向。**重试行为不变**（仍然一律不重试）。
- **非法 UTF-8 的 2xx 响应体现在报 `JsonError`**（此前会被有损替换成 U+FFFD 后「解析成功」）：
  JSON 解析改走 `serde_json::from_slice`，不再先转 `String`。
- **`WechatPay::fetch_platform_keys()` 会丢弃已过期的平台证书**（`x509_is_valid`），
  且 `/v3/certificates` 的应答（含公开的 `WechatPay::certificates()`）一律**自校验**
  （用响应内下发的那张证书验它自己）；返回的仍是全新索引，调用方应整体替换。
- `WechatPay` 的 `Debug` 增加 `response_verify` 与平台证书 serial 列表（都不是机密，
  且是「是不是误关了验签 / 现在认了哪几张证书」的唯一排查线索）。
- **非 2xx 一律返回 `Err`**。此前错误体会被解析进字段全为 `Option` 的成功类型，
  下单失败会伪装成 `Ok(JsapiResponse { code: Some("PARAM_ERROR"), prepay_id: None })`。
- **2xx + 错误信封也返回 `Err`**。微信偶尔以 HTTP 200 返回 `{"code","message","detail"}`，
  现在会被识别（判据：顶层出现非空字符串 `code`；`code: null` / `""` 不算）。
- **HTTP 客户端改为复用**：连接池跨请求共享；此前每个请求都 `Client::new()`。
- **新增超时**：connect 5s / request 10s / pool idle 90s，可用 `with_timeouts` 覆盖。
  此前没有超时，服务端不响应会一直挂住。
- **回调验签新增防重放**：`PlatformKeys::verify_notify` 校验 ±300s 时间戳窗口。
  此前 `verify_signature` 只验签名，抓到一次合法回调即可无限重放。
- **平台证书按 `Wechatpay-Serial` 选键**：轮换期新旧两张并存，不再只认单张
  （此前轮换期会导致回调验签全部失败 → 订单不发货）。
- 格式错误的 2xx 响应体现在归类为 `JsonError`（此前是 `RequestError`）。

### 新增

- **`WechatPayRefundDecodeData` + `PayNotifyTrait::decrypt_refund_paydata`**：退款结果通知的
  专用解密入口。退款通知的字段与支付通知不重合（没有 `appid` / `trade_state`），此前用
  `decrypt_paydata` 处理它一律报「缺字段 `appid`」。
- **`WechatPayConfig`**（具名字段的凭据结构体）与 **`PackagePrefix`**（`wx.requestPayment`
  的 `package` 前缀枚举）。
- `util::now_unix_secs()` 等时间助手（`chrono` 依赖随之移除）。
- **自动重试**（默认开启）：按「这次失败在微信侧到底有没有被处理」分类，只重试
  **确定没送到**（连接被拒 / 连接超时）、**微信明确未受理**（429 / 500 / 502 / 503，
  以及错误码为 `SYSTEM_ERROR` 的 2xx 错误信封）与**已受理但未处理完**（202）的失败。
  措辞取自官方 HTTP 状态码页：429「请求未受理」、502/503「请求无法处理」、
  各接口的 500「请用相同参数重新调用」、202「请使用原参数重复请求一遍」。见 `src/retry.rs`。
- `RetryPolicy`（次数 / 退避 / 抖动）+ `WechatPay::with_retry`。退款走**独立**的分钟级策略
  （`RetryPolicy::for_refund`：最多 2 次尝试、首次退避 60s、**不加抖动**）—— 官方对退款
  重试的要求是「间隔 1 分钟」，且该接口在**失败时**报错限流只有 6QPS，秒级退避打过去基本
  是白打；这一档关掉抖动是因为抖动**向下**取随机，等于把「至少间隔 1 分钟」变成
  「可能立刻重发」。用 `with_refund_retry` 单独覆盖或关闭。
- `PayError::may_have_taken_effect()`：区分「结果未知 / 可能已生效」（读写超时、响应体没读
  完、202、响应体解析失败）与「确定没生效」，供调用方决定是否该去查单，而不是换个单号重开。
- **出站应答验签（默认强制）**：微信不只给回调签名，**每个应答**都带 `Wechatpay-Serial` /
  `-Timestamp` / `-Nonce` / `-Signature` 四个头（验签串 `{时间戳}\n{随机串}\n{应答体}\n`，
  用的是同一批平台证书 / 微信支付公钥）。此前 SDK **完全不看应答签名**，而调用方也无法补偿
  （端点方法只返回已解析类型，拿不到响应头）—— 一个能改应答的中间层可以伪造
  `ORDER_NOT_EXIST`、「请求未受理」这类结论，诱导调用方做出错误处置。
  现在验签在**错误归一之前**完成：伪造的 4xx 信封不会被当成微信的业务拒绝。
  实现要点：`send_and_check` 改为返回「状态码 + 响应头 + **原始字节** body」，
  JSON 解析走 `from_slice`；密钥索引由 SDK 自己维护（首个请求前自动拉取、12 小时窗口刷新、
  未知 serial 刷新一次后重验、`/v3/certificates` 自校验、单飞 + 60s 限流）。
- **`ResponseVerify` + `WechatPayConfig::response_verify` + `WechatPay::with_response_verify`**：
  `Required`（推荐，也是唯一该在生产用的值） / `Disabled`（仅 mock 网关与离线测试）。
- **`WechatPay::with_platform_public_key(id, pem)`**：微信支付公钥模式（`PUB_KEY_ID_…`）
  的支持 —— 该公钥不在平台证书列表里，必须显式配置；配置后客户端进入**静态密钥模式**
  （不自动拉取、不自动替换）。
- **`WechatPay::platform_keys()` / `set_platform_keys()` / `refresh_platform_keys()` /
  `refresh_platform_keys_for_unknown_serial(serial)`**：查看当前索引快照、灌入固定证书
  （测试 / 自建密钥源）、手动刷新，以及**回调轮换兜底**用的限流刷新入口。
- **`cert::UNKNOWN_SERIAL_REFRESH_MIN_INTERVAL_SECS`**（60s）：未知 serial 触发的刷新限流 ——
  伪造一个 serial 就能触发刷新，而该接口官方要求 12 小时一次。窗口只在刷新**成功**后计
  （失败会尽快重试，不会把随后一分钟的验签全拖死）。
- 离线用例从 51 增到 73：应答验签的正向 / 篡改 / 超窗（含 `i64::MIN`）/ 缺头（2xx、4xx、204、5xx）
  / 冷启动自校验 / 轮换自锁 / 刷新后仍未知 / 刷新失败不重发 / 公钥模式 / 并发单飞（含冷启动）
  / 静态密钥不被飞行中的拉取覆盖 / 关闭开关，以及「写接口坏签名只发一次且可能已生效」。
- **`PayNotifyTrait::decrypt_notify` / `decrypt_refund_notify` + `WechatPayNotifySource::validate_algorithm`**：
  带 `resource.algorithm` 校验的回调解密入口（非 `AEAD_AES_256_GCM` 在解密前返回 `DecryptError`，
  而不是退化成 GCM 解密失败）。旧的 `decrypt_paydata` / `decrypt_refund_paydata` 行为不变。
- **`WechatPay::with_key_refresh_wait(Duration)` + `cert::DEFAULT_KEY_REFRESH_WAIT`**：
  平台证书单飞刷新的等待预算，默认仍是 2 秒（旧行为），`Duration::ZERO` 表示不等待。
- **`WechatPay::with_base_urls(primary, backups)`**：主 + 备网关地址，每次重试轮换到下一个
  （官方跨城冗灾）；host 不参与签名，换域名重发与同域名重试的重放语义一致；
  不配置备域名时行为与原先逐字节一致。
- **`WechatPay::add_static_platform_key(serial, pem) -> Result<bool, PayError>`**：向**静态密钥表**
  注入一把公钥（典型是微信支付公钥），**不改变模式** —— 平台证书的自动拉取 / 12 小时刷新 /
  未知 serial 刷新照旧。同一个客户端因此能同时验证「静态公钥」与「轮换中的平台证书」签名的
  应答与回调，这正是官方**微信支付公钥 ⇄ 平台证书灰度切换期**的要求（切换期间同一商户的报文
  可能由两者随机签名）。PEM 当场解析（坏 PEM 立即返回 `VerifyError`，不等到第一笔回调），
  返回 `Ok(true)` 表示覆盖了静态表里同 serial 的旧条目。
  与 `with_platform_public_key`（注入 + 切静态模式）的区别只在于**模式是否被切走**。
- **`PlatformKeys` 变为双表**（`injected` 调用方注入 + `rotating` 自动拉取的平台证书）：
  新增 `PlatformKeys::insert` 写入注入表、`add_static_platform_key` 为注入入口；选键
  **静态表优先**（同一 serial 两表都有时用注入的那把）；`len` / `serials` / `is_empty`
  报告两表合并（去重）的结果。`PlatformKeys::get` / `verify` 因此对回调与应答同时可见两表。
- 离线用例从 73 增到 84：公钥与平台证书两种签名在同一客户端内各自验签通过、注入公钥后
  **冷启动不预拉证书**、按需（未知 serial）拉一次证书后重验、显式刷新仍可用（模式未被切走）、
  `set_platform_keys` 仍整体替换并切静态模式、未知公钥 ID 连一次证书请求都不触发，
  以及「轮换表刷新失败但有静态密钥时请求照发」。
- **`WechatPay::trade_bill(bill_date, bill_type, tar_type)` + `WechatPay::download_bill(download_url)`**：
  交易账单（对账用）的两个端点。申请账单走常规**已验签** GET（`GET /v3/bill/tradebill`，
  `bill_date=yyyy-MM-dd`、`bill_type` ∈ `ALL/SUCCESS/REFUND`、`tar_type=GZIP`），
  返回 `{ hash_type, hash_value, download_url }`（构造 `TradeBillResponse`，配套
  `model::BillType` / `model::BillTarType`）。⚠ 两条官方规则与其它端点**相反**，已在文档里写明：
  ① 账单**下载**应答不带签名头（官方明文），因此 `download_bill` **跳过应答验签**、也不做
  `ensure_keys()`（不需要平台密钥）—— 完整性必须由调用方比对 `hash_value`（官方 `hash_type`
  固定 **SHA1**，不是 SHA256）；② 下载地址是**绝对的**，若按 `base_url` 前缀拼接会发到别的主机，
  所以按官方给的地址**原样**请求（签名串第二行用它的 `path?query`），并只接受 `/v3/` 之下、
  默认只接受 https（只有调用方把客户端指向明文网关时才放行 http）。
  `download_bill` 返回**原始字节**（`tar_type=GZIP` 时是 gzip 流，由调用方解压），
  非 2xx 以**未验签**的 `ApiError` 返回（消息带 `[未验签]`）。
- 订单查询 `query_order`、关单 `close_order`、退款查询 `query_refund`。
- 平台证书管理：`cert::PlatformKeys` + `WechatPay::fetch_platform_keys`（含刷新窗口）。
- 回调防护：`notify::NotifyHeaders`（含 `from_pairs`）+ `PlatformKeys::verify_notify`。
- 错误三层归类：`PayError::kind() -> ErrorKind`（Network / Api / Local）。
- `HttpTimeouts` 与 `WechatPay::with_timeouts` / `timeouts()`。
- 离线测试网 `tests/offline.rs`（30 个带断言的用例，不需要凭证）+ GitHub Actions CI。
- 归属声明 `NOTICE`、本 `CHANGELOG.md`。

### 修复

- **重写 `example/` 的回调服务器**：原来三个 handler 里，注册进 `App` 的 `pay_notify`
  **完全不验签**（只把 body 写进文件就返回成功 —— 照抄等于接受伪造的「支付成功」通知），
  `pay_notify2` 只解密不验签，而唯一做过验签的 `pay_notify3` **没有注册**（死代码）。
  现在是一条完整链路：`NotifyHeaders::from_pairs` → `PlatformKeys::verify_notify` →
  `decrypt_paydata` → 幂等占位 → 204 应答，并注明「拿到 `UnknownPlatformSerial` 要立即重拉证书」。
  同时**去掉示例的 `debug-print` feature**：它会打印 Authorization 头与请求体（含 openid、金额），
  而示例会被当成生产范本抄走。
- **README 两处回调小节补上警告**：`## 支付回调解密` 与 `## actix-web demo` 只演示「只解密、不验签」，
  现在明确标注「解密不能替代验签」并指向正确流程 —— 解密（AEAD）能挡住伪造的密文，
  但挡不住**重放**（抓到一次合法回调即可反复投递）。
- **README 的「小程序支付」小节改用 `jsapi_pay`**：原先指向 `micro_pay`（该方法发往 JSAPI 下单端点
  且 `MicroParams` 缺付款码支付必需的 `auth_code`），而且示例把 `JsapiParams` 传给了要求 `MicroParams`
  的参数 —— 那段示例根本编译不过。新示例补上 `openid` 的来源说明与 `sign_data` → `wx.requestPayment`
  的映射，并说明「小程序支付就是 JSAPI，与公众号 JSAPI 只差 openid 来源」。
- **回调验签的时间戳偏差改用无符号距离**：`Wechatpay-Timestamp` 取 `i64::MIN` 时
  `now - signed_at` 会算术溢出 —— 开启 overflow-checks 的构建里直接 panic（未鉴权的请求头即可触发），
  release 构建下则会回绕并**放行**这个明显伪造的时间戳。现在按 `abs_diff` 比较，两种情况都稳定
  判为 `StaleNotify`，并补了极值回归用例。
- **支付通知缺 `attach` 时不再解析失败**（见上面的 `Option` 改动），并补了回归用例。
- **`code_url` / `h5_url` 的文档注释原先互换了**（二维码字段挂着 H5 的说明、H5 字段挂着二维码的
  说明）—— 已对调，`【…】` 标签一并修正。
- **`JsapiParams::new` 的文档写着「构造 NativeParams」**；**`PayType::Display` 输出的
  `MICRO` / `H5` / `QRCODE` 与官方 `trade_type`（`MICROPAY` / `MWEB` / `NATIVE`）不一致** —— 均已修正。
- **`cargo doc` 的 15 处断链 + 1 处指向私有项的公开文档链接已修**，CI 新增
  `cargo doc --no-deps --all-features`（`RUSTDOCFLAGS: -D warnings`）门禁防止复发。
- 删掉两个不构成回归保护的用例（`test_uuid_v4` 只打印、`test_str` 无断言），并补上
  `PlatformKeys::verify_notify`（真实墙钟路径）、`random_trade_no`、抖动分布与
  `WechatPayConfig` 脱敏的用例。
- **平台证书到期过滤**：`GET /v3/certificates` 返回的已过期证书此前会被照单收进索引
  （轮换期新旧并存时尤其危险：留着过期证书会让「拿别的密钥去试」看起来可行）。
- **`util::x509_is_valid` 的返回类型由 `Box<dyn Error>` 改为 `PayError`**（公开签名变更）：
  与库其余错误类型一致，调用方可以直接 `?`。
- **`/v3/certificates` 的应答不再可能「自锁」**：轮换期该应答由**新**证书签名，而本地索引里
  还没有新证书 —— 若按普通端点严格验签，刷新请求自己就验不过，永远学不到新证书。
  现在这条应答一律用响应内下发的证书自校验（唯一豁免，且不限「索引为空」，
  公开的 `certificates()` 也走同一条路）。
- **刷新失败不再污染业务请求的失败分类**：证书刷新的错误来自**另一条请求**，
  此前会以原类型逃进 `classify()` —— 连接类 / 429 / 5xx 会被判成「可重试」，
  导致**重发那条应答已经收到的写请求**；4xx 又会让调用方拿到「确定没受理」。
  现在统一归一成 `UnknownPlatformSerial`（不重试 + 「可能已生效」）。
- **刷新标记改为 RAII 释放**：异步任务被 `timeout` / `select!` / abort 丢弃时，
  原先的手动复位不会执行，`in_flight` 会永久为真 —— 之后所有需要刷新的请求都会失败。
- **单飞覆盖全部刷新路径**（原先只有「未知 serial」那条）：冷启动与 12 小时窗口到期的瞬间，
  N 个并发请求会各自打一次 `/v3/certificates`（官方要求 12 小时一次的接口）。
- **安装拉取结果前复查静态密钥模式**：与一次飞行中的拉取撞上时，`set_platform_keys` /
  `with_platform_public_key` 灌入的密钥会被**不可逆地**覆盖（静态模式下不再自动拉取）。
- **应答路径的时间戳极值不再有溢出风险**：与回调共用 `check_timestamp_skew`（`abs_diff`）。
- **非 UTF-8 的 2xx 错误信封仍会被识别**：信封判据先吃原始字节、失败再退回有损文本，
  免得关单那条路（`request_no_content` 不解析 JSON）把这种应答当成成功。
- **`example/` 的证书索引改为用 SDK 那份**：示例原先自己 `fetch_platform_keys` 出一份
  `PlatformKeys` 放进 `App`，与 SDK 内部那份**互不同步** —— 轮换后回调那条链路会用旧索引。
  现在：启动用 `refresh_platform_keys()` 预热（顺带 fail-fast 暴露凭证问题），回调里取
  `platform_keys()` 快照，拿到 `UnknownPlatformSerial` 就
  `refresh_platform_keys_for_unknown_serial()`（带 60s 限流，避免未鉴权的伪造 serial
  把证书接口变成放大器）后重验。
- **`set_platform_keys` 与在途拉取之间的竞态**：置位 `static_keys` 与写索引原先不在同一个
  写锁临界区，留下「索引已是调用方密钥、标记还说不是静态模式」的窗口 —— 此刻一次在途的
  拉取会通过复查并**覆盖用户密钥**（静态模式下不会再有自动拉取 ⇒ 公钥模式永久失效）。
- **限流判断与单飞抢标记的顺序反了**：先读 `in_flight` 再判断限流，等待者会读到别人刚写入的
  时间戳，被误判成「刚刷新过」而拿到虚假的 `UnknownPlatformSerial`。现在抢到标记的人才做
  限流判断，读写落在同一个临界区。
- **限流窗口不再被失败的刷新烧掉**：一次瞬时失败（网络抖动、上游 5xx）原先会让随后 60s 内
  所有未知 serial 请求都失败；现在窗口只在**成功**后计，失败会尽快重试。
- **证书列表路径上的非 2xx 应答也会标 `[未验签]`**：那条路径的 2xx 由调用方自校验，
  错误体的签名从头到尾没验过 —— 不该让调用方以为它是可信的。
- **错误吞噬**：下单失败曾被当作成功返回。
- **非微信形状的错误体被静默丢弃**：现在保留原文（超长截断、空体有显式标记）。
- **许可证元数据不一致**：`Cargo.toml` 声明 MIT 而 `LICENSE` 是 Apache-2.0 ——
  已统一为 Apache-2.0（代码实际收到的许可），并补齐版权署名。
- **`cargo check --features async --all-targets` 编译失败**：7 个测试函数缺少 cfg 门。
- **16 个冒烟测试在干净检出上必然失败**：已标记 `#[ignore]`，`cargo test` 转绿。

### 其他

- 声明 MSRV **1.89**。`rust-version` 同时让 cargo 优先挑选兼容该版本的依赖 ——
  本仓库不提交 `Cargo.lock`，不声明的话实际 MSRV 会随解析漂移（实测 1.85 / 1.88 均失败）。
- **删除 22 处冗余 `unsafe impl Send/Sync`**，并加 `#![forbid(unsafe_code)]` 与编译期
  `Send + Sync` 断言。
- 公开 API 全部补齐文档注释（原缺 179 处），并以 `missing_docs` + CI `-D warnings` 作硬门槛。
- 全仓 rustfmt 化，CI 增加 `cargo fmt --all -- --check`。
