//! Parse independent J++ source. Execution is delegated to the shared core.
pub mod ast;
pub mod diagnostic;
mod lexer;
pub mod loader;
mod lower;
mod parser;

pub use diagnostic::Diagnostic;
pub use lower::lower;
pub use parser::parse;
