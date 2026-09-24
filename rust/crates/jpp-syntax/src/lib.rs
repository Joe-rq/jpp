//! J++ 的 L1 层（`20` §2.3「L1 · `jpp-syntax`」）：词法、语法、表层 AST、多文件装载，以及把表层
//! AST 直接降到 IR 的 [`lower`] 模块。步 12d 由 `jpp-frontend` 改名。
//!
//! `lex` / `parse` 不引用 `jpp_ir`（文法层不承担语义识别）；只有 `lower` 模块读 IR 与名字表。
pub mod ast;
pub mod diagnostic;
mod lexer;
pub mod loader;
pub mod lower;
mod parser;

pub use diagnostic::Diagnostic;
pub use lower::lower;
pub use parser::parse;
