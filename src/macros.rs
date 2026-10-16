//! crate 内部的日志宏。
//!
//! `debug!` 有两份定义，由 `debug-print` feature 选择编译哪一份：启用时转发给
//! `tracing`，未启用时展开为空操作（参数不会被求值）。因此默认构建下不会输出任何日志，
//! 也就不会把请求体、签名串这类敏感内容写出去。
//!
//! 用 `pub(crate) use` 而不是 `#[macro_export]`：后者的可见性不受 `pub(crate) mod`
//! 约束，会把内部宏发布到 crate 根 —— 等于把这份临时语法、以及将来的任何改动都冻进
//! 公开 API，还会在 glob 导入时遮蔽同名的 `tracing::debug!` / `log::error!`。

/// 记录调试日志（需启用 `debug-print`；否则编译为下方的空实现）。
///
/// 开启后会输出请求体、Authorization 头与被签名的原文，含 openid、订单号、金额 ——
/// ⚠ 生产环境不要开启。
#[cfg(feature = "debug-print")]
macro_rules! debug {
    ($name:expr)=>{
        tracing::debug!($name);
    };
    ($name:expr $(,$arg:tt)+) => {
        tracing::debug!($name, $($arg),*);
    };
}

/// `debug!` 的空实现：未启用 `debug-print` 时什么都不做，参数不求值。
#[cfg(not(feature = "debug-print"))]
macro_rules! debug {
    ($name:expr) => {};
    ($name:expr $(,$arg:tt)+ $(,)?) => {};
}

pub(crate) use debug;
