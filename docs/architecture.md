# Aether 架构设计

> 对应路线图各里程碑;本文件随实现演进,重大变更写 ADR。

## 一、四层模型(L4–L1)

| 层 | 名称 | 职责 | MVP 载体 → 远期载体 |
| :--- | :--- | :--- | :--- |
| **L4** | 意图层 | 自然语言 → 意图结构 | 首版 = LLM 直接生成 Aether 源码(M4 harness);远期 = 意图矩阵 |
| **L3** | 核心层 | Aether 源码 ↔ AST(同像) | 文本源码(首版)→ 二进制 AST(远期) |
| **L2** | 编译验证层 | parse → typecheck → 契约验证 → 后端选择 | `aether-parser` / `aether-diagnostic` / `aether-verify`(Z3) |
| **L1** | 执行层 | 解释 / 原生码 | `aether-interp`(M2)→ 性能后端(M5) |

## 二、编译流水线

```text
source text (.ae)
   │  lexer ── 词法(tokens,含 span)
   ▼
parser ── 递归下降,生成 AST(每个节点带 NodeId + Span)
   │
   ▼
typecheck(M3)── 双向类型检查,产出带类型标注的 AST
   │
   ▼
contract verify(M3)── pre/post/invariant:
   │                   运行期检查器(兜底)+ Z3 静态验证(数值/数组域)
   ▼
后端选择 ── interp(M2)│ codegen(M5)
```

- **NodeId 全链路贯穿**:诊断、补丁、指标都以 NodeId 为锚点。
- **诊断模型**:`{ node_id, span, severity, code, message, hints[] }`,支持 JSON 序列化(M4 直接喂回 LLM)。

## 三、Workspace(crate)划分与依赖图

```text
aether-ast        (AST/NodeId/Span,无依赖)
   ▲
aether-diagnostic (结构化诊断;依赖 ast)
   ▲
aether-parser     (lexer/parser/printer;依赖 ast + diagnostic)
   ▲
aether-verify     (typecheck + contracts + Z3;依赖 ast + diagnostic)
   ▲
aether-interp     (运行时;依赖 ast + diagnostic)
   ▲
aether-cli        (parse/check/run/repl 子命令;聚合上述)
   │
aether-metrics    (M4:token/指标;独立)
```

- 依赖方向单向、禁止循环;新增 crate 须同步更新本图与根 `Cargo.toml`。
- 每个 crate 自带单测;跨 crate 集成测试放根 `tests/`。

## 四、关键数据模型(草图,随 M1 冻结)

- **NodeId**:模块内全局唯一自增 ID;诊断与图补丁的锚点。
- **AST 核心节点**(v0.1 子集):`Lit / Var / Let / Fn / Call / If / Block / Struct / ContractBlock{pre,post,inv} / Module`。
- **值模型**:不可变值 + Rc;「状态流(内容寻址 ID)」在 M2 仅作实验,不进 v0.1 承诺。
- **契约表示**:每个契约是「约束表达式」,运行期求值为布尔,Z3 通道翻译为 SMT 断言。

## 五、技术底座(不重复造轮子)

| 用途 | 选型 | 里程碑 |
| :--- | :--- | :--- |
| 实现语言 | Rust 1.98(workspace) | ADR-0001 |
| 静态验证 | Z3(z3 crate,SMT 求解) | M3 |
| 性能后端 | 字节码 VM / inkwell(LLVM)/ C 转译 —— 三选一,ADR 决策 | M5 |
| 生成回路 | 结构化诊断 JSON + LLM API | M4 |
| 经验借鉴 | KARN(token 压缩)、Zero(图原生)、IF-Lang(意图优先)、NanoLang/Synoema(形式验证)——**只借鉴思路,不复制代码** | 全程 |

## 六、演进纪律

1. 语言行为变更:先改 `docs/spec/`,再改实现,两者同步提交。
2. 架构级取舍(依赖方向、后端选型、数据模型):写 ADR。
3. 性能主张:只引用 `docs/bench/` 里的实测数据,禁止无数据口号。
