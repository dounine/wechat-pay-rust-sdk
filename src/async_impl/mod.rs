//! HTTP 实现层。
//!
//! ⚠ 模块名有点误导：这里**同时**提供同步与异步 API —— `maybe-async` 在编译期把同一份
//! `pub async fn` 改写为同步函数（默认），或保留异步（`async` feature）。所以它是唯一的
//! HTTP 实现，不要在这里找「只有异步」的东西。

/// 所有端点与请求编排（`send_and_check` → `request*` → 各端点）。
pub mod pay;
