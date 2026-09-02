//! Aether 字节码 VM(M5,ADR-0002 方案 A)。
//!
//! 语义与树遍历解释器一致(02-semantics / 06-std);
//! 与 aether-verify 组合成「类型检查 → 静态契约验证 → VM 执行」的完整流水线。

pub mod compiler;
pub mod opcode;
pub mod vm;

pub use vm::Vm;

use aether_diagnostic::Diagnostic;

/// 编译 + 运行程序的便捷入口:返回进程退出码。
pub fn run(program: &aether_ast::Program, argv: Vec<String>) -> Result<i32, Diagnostic> {
    let mut vm = Vm::from_program(program)?;
    vm.run_main(argv)
}
