//! 校准记录、样本、证书与 `CalibStore`（线只从记录来）。（步 4c 自 `effects.rs` 原样搬出）
//!
//! 2026-09-24 按职责拆成四个文件，只搬不改（`20` 每文件不超过 800 行）：`record`（记录、样本、
//! 证书、标注来源）、`store`（`CalibStore` 本体与写入口）、`commission`（认证与选线）、
//! `access`（漂移、查询、持久化、键构造、读线）。公共路径不变。

mod access;
mod commission;
mod record;
mod store;

pub use record::*;
use record::{单侧声明, 反查题型, 标注集指纹, 经验unsure率};
pub use store::*;
