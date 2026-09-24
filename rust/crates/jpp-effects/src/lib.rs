//! 效应本体（`20` §2.3 `jpp-effects`，L2）：一种效应「是什么」——输入槽、输出形状、键的组成、
//! taint 规则、调度类别、是否产出读数——以及能力画像的类型与只读视图 trait。
//!
//! 步 9（R，`21` §三·4）只抽类型部分：
//! - `spec`：`EffectSpec` 注册表，每种效应一文件 `kinds/<name>.rs`。**分派仍走 `jpp-core` 的旧代码**，
//!   六处改读 `EffectSpec` 字段在步 15a。
//! - `profile`：`Profile` 与档案哈希，字段暂与现状一致（按效应实例分表、删 `Profile::default` 在步 15d）。
//! - `views`：`CalibView`/`FitView`/`MatStorePort`/`CacheLookup`。本步没有实现者；`CalibStore` 实现
//!   `CalibView` 在步 11，料库与缓存在步 18、19。
//! - 旧 `Client` trait 留在 `jpp-core`，经默认方法 `Client::instance` 接到 `EffectInstance`；
//!   统一端口 `EffectPort` 在步 15b。
//!
//! 禁止依赖（`20` §2.3）：`jpp-runtime`、`jpp-calib`、`jpp-ledger`、HTTP。

pub mod kinds;
pub mod profile;
pub mod spec;
pub mod view;
pub mod views;

pub use jpp_ir::key::{EffectId, EffectInstance};
pub use kinds::{ALL, CLIENT_SERVED, spec};
pub use profile::{Profile, Tri, behavior_hash, hash16, profile_hash};
pub use spec::*;
pub use views::*;
