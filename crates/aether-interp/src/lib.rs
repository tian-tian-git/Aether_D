//! Aether 树遍历解释器(M2)。
//!
//! 规范:docs/spec/v0.1/02-semantics.md、06-std.md。

pub mod builtins;
pub mod eval;
pub mod value;

pub use eval::Interp;
pub use value::Value;

use std::rc::Rc;

use aether_ast::{Expr, StructDef};
use aether_diagnostic::Diagnostic;

/// 树遍历解释器的内建宿主(供 builtins 回调)。
impl builtins::Host for Interp {
    fn call_fn_value(&self, f: &Rc<value::FnValue>, args: Vec<Value>, call_site: &Expr) -> Result<Value, Diagnostic> {
        Interp::call_fn_value(self, f, args, call_site)
    }

    fn struct_def(&self, name: &str) -> Option<Rc<StructDef>> {
        self.struct_def(name)
    }
}
