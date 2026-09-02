# Aether —— AI 原生语言(认知执行协议 CEP)

> 一门**为 AI 而生、而非为人类而生**的编程语言:AI 直接生成、契约验证、可自举演进。

## 一句话定位

当前 AI 写代码的模式是「AI → 人类语法(Python/C++)→ 编译器 → 机器码」,中间夹着一层为人类设计的语言——充满歧义、隐式规则与历史包袱。**Aether 去掉这层夹层**:AI 直接输出逻辑拓扑(结构化 AST),由编译器完成验证(契约 / SMT)与降级(解释 / 编译),把大模型从「概率性代码复制器」推向「确定性逻辑证明器」。

我们**不从头造轮子**:站在 LLVM / MLIR / Z3 与既有 AI 语言项目(KARN、Zero、IF-Lang 等)的经验之上,先做「AI 与现有硬件之间的超薄语义胶水层」,逐级演进。

## 核心设计支柱

1. **无歧义语法(AST 即语法)**:S 表达式风格,无运算符优先级、无隐式转换;语法树与语义树重合,Token 消耗最小化。
2. **契约驱动(Design by Contract)**:`pre` / `post` / `invariant` 是一等语法;运行时检查兜底,接入 Z3 做数值域静态验证。
3. **默认不可变 + 显式效果**:值语义、纯函数默认,副作用必须显式声明;目标是消灭「未定义行为」。
4. **同像性(Homoiconicity)**:代码即数据,支持反射与结构级补丁——AI 改节点,而不是改文本。
5. **AI 优先的工具链**:结构化诊断(节点 ID + 契约 ID + 修复建议)、生成—验证—修补闭环、Token 指标是一等公民。

## 务实边界(明确不做什么)

- 不推翻冯·诺依曼硬件,不与 Python / C++ 全面竞争:我们是「AI 与现有硬件之间的语义胶水层」。
- 不追求人类可读性;人类通过「反渲染器」(远期)审查代码。
- 不一步到位 MLIR / LLVM:先树遍历解释器锁定语义,再逐级加性能后端。

## 仓库结构

```
Aether_D/
├── README.md                # 本文件
├── AGENTS.md                # 智能体操作手册(所有参与 Agent 开工前必读)
├── AI跨语言设计.txt          # 原始设计对话(只读档案,背景文献)
├── docs/
│   ├── vision.md            # 愿景与设计支柱
│   ├── roadmap.md           # 里程碑路线图(以退出标准推进,不用日历时间)
│   ├── architecture.md      # 四层架构与编译流水线
│   ├── spec/v0.1/           # 语言规范(草案 → M1 冻结)
│   └── adr/                 # 架构决策记录
├── crates/                  # Rust workspace(子项目)
│   ├── aether-ast/          # AST / 数据结构(M1 填充)
│   └── aether-cli/          # 命令行入口
├── examples/                # Aether 示例程序
└── tools/                   # 生成回路、指标等辅助工具(后续里程碑)
```

## 当前状态

- **M0 立项与框架** ✅
- **M1 语法与解析** ✅(spec 候选冻结稿 + 词法/解析/打印/诊断/CLI 全部就绪,待人类总监门禁确认)—— 详见 [docs/roadmap.md](docs/roadmap.md)

## 快速开始

```powershell
cargo build
cargo run -p aether-cli -- parse examples/hello.ae          # 解析并 dump AST
cargo run -p aether-cli -- parse 你的文件.ae --json          # 结构化诊断 JSON(供 AI 生成回路)
cargo test                                                   # 57 项测试
```

## 工作方式

所有参与本项目的 AI 智能体,开工前必读 [AGENTS.md](AGENTS.md)。

## 硬规则

> **绝对禁止对 `D:\Desktop\programme\ai-lp\Aether_K` 进行任何读取 / 修改 / 执行操作。** 一切工作仅在 `Aether_D` 内进行。

## 背景文献

原始设计对话与调研:[AI跨语言设计.txt](./AI跨语言设计.txt)(只读档案,勿修改)。
