//! Aether 静态验证(M3):类型检查 + 契约验证。
//!
//! 规范:docs/spec/v0.1/03-types.md、04-contracts.md。

pub mod typecheck;
pub mod z3bridge;
pub mod z3ffi;

pub use typecheck::check_program;
pub use z3bridge::{try_load, verify_program};
