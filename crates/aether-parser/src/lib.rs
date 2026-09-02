//! Aether 前端:词法 → 解析 → 打印。
//!
//! 规范:docs/spec/v0.1/01-syntax.md。
//! 当前进度(M1):词法分析器、递归下降解析器已完成;打印器(WP1.4)随后接入。

pub mod lexer;
pub mod parser;

pub use parser::parse;
