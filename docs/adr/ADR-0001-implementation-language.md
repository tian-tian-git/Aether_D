# ADR-0001:参考实现选用 Rust

- **状态**:已接受
- **决策记录于**:M0
- **决策人**:人类总监(选定)+ 主智能体(推荐)

## 背景

Aether 的参考实现(解析器/解释器/编译器)需要一种实现语言,长期要求:性能(最终有原生后端目标)、与 LLVM/MLIR/Z3 的绑定生态、内存安全(解释器持有复杂对象图)、便于 workspace 拆分为多个子项目以支持智能体并行协作。

## 选项

| 方案 | 优势 | 劣势 |
| :--- | :--- | :--- |
| **A. Rust** | 内存安全;`z3`/`inkwell`(LLVM)绑定成熟;cargo workspace 天然支持「拆分项目、分布执行」;性能无返工风险 | 编译迭代稍慢;参与者需熟悉 Rust |
| B. Python 3.13 | 零安装、迭代最快 | 运行慢;核心后期需用 Rust/LLVM 重写一遍 |
| C. TypeScript/Node | 迭代快、类型系统强 | LLVM/MLIR 生态最弱,长期需换栈 |

## 决策

选 **A. Rust**(机器上已有 Rust 1.98 stable-x86_64-pc-windows-gnu,位于用户级 `~/.cargo`,无需系统级改动)。

## 后果

- **正面**:workspace 按 crate 拆分(aether-ast/parser/verify/interp/cli/metrics)直接支撑路线图的并行工作包;Z3、LLVM 接入路径畅通。
- **负面与缓解**:编译时长 → crate 增量编译 + 先 `cargo check` 后 `cargo test`;学习成本 → `AGENTS.md` 工程纪律 + 子智能体按文件边界分工。
- **工具链约定**:本会话 PATH 快照可能不含 `~/.cargo/bin`,调用 cargo 时使用全路径或临时前缀 `$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path`。
