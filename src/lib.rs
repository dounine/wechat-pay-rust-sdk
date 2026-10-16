#![doc = include_str!("../README.md")]
// 本 crate 不含任何 unsafe 代码：所有类型依赖自动派生的 Send/Sync，
// 原先的 22 处 `unsafe impl Send/Sync` 已删除。
// 编译期由 tests/offline.rs::public_types_are_send_and_sync 守住这个性质。
#![forbid(unsafe_code)]
/// HTTP 实现层：`maybe-async` 在编译期把同一份源码改写为同步或异步 API。
pub mod async_impl;
/// 平台证书 / 微信支付公钥的按键索引：轮换期按 `Wechatpay-Serial` 选键。
///
/// 见 [`PlatformKeys`](crate::cert::PlatformKeys)。
pub mod cert;
/// 错误类型 [`PayError`](crate::error::PayError) 与三层归类
/// [`ErrorKind`](crate::error::ErrorKind)。
pub mod error;
pub(crate) mod macros;
/// 请求参数与回调通知模型（`Serialize`）。
pub mod model;
/// 回调通知的验签与防重放：新鲜度 → 选键 → 验签。
pub mod notify;
/// 客户端 [`WechatPay`](crate::pay::WechatPay)，以及签名 / 解密 / 验签相关的 trait。
pub mod pay;
/// 支付方式枚举。注意：crate 内部并未使用它。
pub mod pay_type;
/// HTTP 方法枚举。
pub mod request;
/// 响应模型（`Deserialize`）。
pub mod response;
/// 自动重试策略 [`RetryPolicy`](crate::retry::RetryPolicy)：哪些失败能重试、退避多久。
pub mod retry;
/// RSA-SHA256（PKCS#1 v1.5）签名。
pub mod sign;
/// Base64、随机单号、X.509 解析与 RSA 验签等公开助手。
pub mod util;
