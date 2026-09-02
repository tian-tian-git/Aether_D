//! Aether 前端:词法 → 解析 → 打印。
//!
//! 规范:docs/spec/v0.1/01-syntax.md。
//! M1 已全部完成:词法分析器、递归下降解析器、规范打印器。

pub mod lexer;
pub mod parser;
pub mod printer;

pub use parser::parse;
pub use printer::print_program;
