//! 效应适配：`Client`（观察 = 模型调用或固定记录）、校准记录（线只从记录来）。

mod client;
mod jev;
mod profile;

// 校准记录、校准库与 fit 注册表在步 11 搬进 `jpp-calib`，这里原路径重导出。
pub use jpp_calib::calib::*;
pub use client::*;
pub use jpp_calib::fit::*;
pub use jev::*;
pub use profile::*;
