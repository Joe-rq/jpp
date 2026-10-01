//! 端到端（`20` v2 附录 B55 条「`tests/e2e/resume.rs` 加中断用例」；`21` 步 18 注 18b）。
//!
//! - [`resume`]：真进程中断（在不可逆动作的意向之后、结果之前杀进程）与续接；意向写不进存储；
//!   声明幂等的动作；可逆动作；续接写新文件；续接不报 `W-header`（B61）；层末落盘。
//! - [`cache`]：按缓存键复用与两段式（B40、B151，步 19）。
//! - [`ledger_required`]：CLI 对有不可逆 `do` 的程序要求 `--ledger-out`（`E-ledger-required`，
//!   主会话 2026-09-25 对步 18b 的裁定）。

// 伴随题「最缺哪类」是 K 选一：替身经 common::伴随中性judge 给中性读数（Z0398 返修）
mod cache;
#[path = "../common/mod.rs"]
mod common;
mod ledger_required;
mod resume;
