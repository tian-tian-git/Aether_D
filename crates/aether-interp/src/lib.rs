//! Aether 树遍历解释器(M2)。
//!
//! 规范:docs/spec/v0.1/02-semantics.md、06-std.md。

pub mod builtins;
pub mod eval;
pub mod value;

pub use eval::Interp;
pub use value::Value;
