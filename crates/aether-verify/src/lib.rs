//! Aether 静态验证(M3):类型检查 + 契约验证。
//!
//! 规范:docs/spec/v0.1/03-types.md、04-contracts.md。

pub mod typecheck;

pub use typecheck::check_program;
