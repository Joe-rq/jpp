# grep_constants 基线口径

- 计数单位：字面量出现次数（每个匹配计一次），不是含字面量的行数；rustfmt 拆行不改变计数。
- 扫描范围：`crates/` 下除 `jpp-cli` 以外的全部 crate（新增 crate 自动纳入）；只扫 `src/`，`#[cfg(test)]` 之后的行不计。
- 匹配：小数、科学计数法；`concurrency|window|delta|k_limit|latency` 后接数字的赋值。
- 允许名单：`jpp-value/src/stat.rs`、`jpp-calib/src/strength.rs`（旧位置 `jpp-core/src/conformal.rs`、`strength.rs`；步 11 随文件搬家更新）。
- 基线：2026-09-24 在 main `9c4cb28` 上重算，132（jpp-core 119、jpp-effects 7、jpp-frontend 4、jpp-value 2）。旧口径（按行、只扫 jpp-core 与 jpp-frontend）的 92 作废。
