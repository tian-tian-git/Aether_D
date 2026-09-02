# Aether 人类指南:使用与维护

> 读者:人类。目标:学会「指挥 AI 用 Aether 解决问题」+「审查与维护这门语言」。
> 本指南只讲事实与工作流,语言细节以 [spec v0.1](../spec/v0.1/00-overview.md) 为准。
> 给 AI 看的对应文档:[ai-guide.md](ai-guide.md)。

## 0. 先建立正确预期:人类不「写」Aether,人类「指挥」和「审查」

Aether 是给 AI 写的语言(语法为确定性而非可读性设计)。人类在这门语言里的三个角色:

| 角色 | 做什么 | 用什么工具 |
| :--- | :--- | :--- |
| **意图架构师** | 用自然语言描述需求,并给出**可验证的约束**(契约) | `tools/harness/demo_nl.py` |
| **审查者** | 验证 AI 的产出是否真的满足约束 | `aether check` / `aether run` / 反汇编 |
| **维护者** | 维护语言本身(语法、语义、内建、工具链) | 本指南第 4 章 + [AGENTS.md](../../AGENTS.md) |

不要试图手写大段 Aether——那是 AI 的活;人类的价值在「提出什么约束」和「确认什么被证明了」。

## 1. 环境与工具链(本机已就绪)

```powershell
cargo build --release                          # 构建全部工具
cargo test                                     # 18 套件、144 项测试(修改语言后必跑)
```

CLI 子命令一览:

| 命令 | 作用 | 示例 |
| :--- | :--- | :--- |
| `aether parse <file> [--json]` | 解析源码,输出 AST;失败输出结构化诊断 | `aether parse examples/sort.ae` |
| `aether check <file> [--json]` | 类型检查 + **Z3 静态契约验证**(静态证伪时附反例) | `aether check examples/bounds.ae` |
| `aether run <file> [args...]` | 类型检查后编译为字节码并执行(默认 VM;`--interp` 回退树遍历) | `aether run examples/hello.ae` |
| `aether repl` | 交互求值(括号自动续行,`:q` 退出) | — |
| `cargo run -p aether-vm --example disasm -- <src>` | 查看编译器产出的字节码 | — |
| `cargo run -p aether-vm --example graph_patch` | 图补丁/热更新 demo | — |

## 2. 五分钟看懂 Aether(从示例入手)

按顺序读这五个文件,每个都刻意只演示一种核心机制:

| 文件 | 学什么 |
| :--- | :--- |
| `examples/hello.ae` | 模块、`main`、显式效果标记 `!` |
| `examples/fib.ae` | 函数、参数类型、`:pre` 契约、递归(语言没有循环) |
| `examples/sort.ae` | `:post` 契约、`result`、匿名函数、filter/concat |
| `examples/point.ae` | 结构体、`:invariant`、字段访问 `(. p "x")` |
| `examples/bounds.ae` | **静态验证的价值**:`aether check` 会在运行前证明越界并给反例 |

再看 `lib/aether-std.ae`:19 个函数全部用 Aether 自身实现、Z3 验证零违反——这是「合格 Aether 代码」的样子。

## 3. 日常工作流

### 3.1 下达意图(自然语言 → 可执行)

```powershell
python tools/harness/demo_nl.py "写一个函数,返回第 n 个斐波那契数,必须带前置条件契约" --input 10
```

产出三样东西:**AI 生成的源码 → 验证结果 → 执行结果**。让 AI 重试的标准句式:「验证失败,反馈是 <粘贴诊断>,修正后只输出完整代码」。

### 3.2 审查 AI 的产出(人类的核心动作)

审查 = 看**证明了什么**,而不是逐行读代码:

1. `aether check file.ae` —— 类型错误和**可静态证伪的契约违反**都会在这里出现,附反例输入;
2. 契约即需求:检查 AI 是否把你的需求翻译成了契约(`:pre`/`:post`),契约是否**足够强**(见 3.3);
3. `aether run file.ae` —— 用真实数据执行;运行期契约检查(E4xxx)是第二道网;
4. 需要看懂实现时:`aether parse`(规范打印,单行化、无缩进噪音)或反汇编看字节码。

### 3.3 契约思维:人类怎么给「可验证的约束」

给 AI 下需求时,把验收标准写成契约语言:

| 自然语言需求 | 契约化表达 |
| :--- | :--- |
| 「输入不能是负数」 | `:pre (>= n 0)` |
| 「输出必须有序」 | `:post (sorted? result)` |
| 「输出长度不能变」 | `:post (== (len result) (len xs))` |
| 「索引不能越界」 | `:pre (and (>= i 0) (< i (len xs)))` |

两点经验:
- **静态可证域**:`Int`/`Bool`/`(Vec Int)` 上的算术与边界约束可以被 Z3 在运行前证伪(附反例);`sorted?` 等谓词静态不展开、运行期兜底——两者配合使用。
- **契约别写得比实现还强**(否则你自己的程序都过不了验证,M4 的 gcd 案例):契约描述的是「约定」,不是「愿望」。

### 3.4 修补与演进

- 逻辑修补:让 AI 在诊断闭环里修(harness 已内置 3 轮修补);
- 结构修补:图补丁按 NodeId 换节点(见 `graph_patch` demo),不重写整个文件;
- 页面/文本产物:`aether run file.ae > out.html`(v0.1 无文件 IO,重定向是当前标准做法,见 `examples/web-hello.ae`)。

## 4. 维护语言本身(维护者指南)

### 4.1 仓库地图

```
crates/
  aether-ast/          语言的数据模型(AST/NodeId/Span/规范打印)——一切的地基
  aether-diagnostic/   结构化诊断(JSON + 渲染,错误码注册表在 spec 05)
  aether-parser/       词法 + 递归下降解析 + round-trip 测试
  aether-verify/       类型检查器 + Z3 静态契约验证(FFI + 符号执行)
  aether-interp/       树遍历解释器(值模型/内建函数/契约运行时)
  aether-vm/           字节码编译器 + VM(默认执行后端)
  aether-cli/          parse/check/run/repl 入口
lib/aether-std.ae      用 Aether 自身实现的标准库(自举成果)
docs/spec/v0.1/        语言规范六章(唯一权威)
tools/harness/         AI 生成回路、基准、蒸馏实验
```

### 4.2 变更纪律(谁改语言都要遵守)

1. **先 spec 后实现**:任何语法/语义变更,先改 `docs/spec/v0.1/`,再改代码,同一次提交;
2. **架构级取舍写 ADR**(`docs/adr/`);
3. **错误码注册**:新诊断必须先登记到 `05-diagnostics.md` 的区间表,再实现,并配一条锁定消息的测试;
4. **测试先行**:`cargo test` 必须全绿;解析类变更必须有 round-trip 测试;
5. **双后端一致性**:语义变更必须同时过 `aether-interp` 与 `aether-vm` 两套测试(它们的用例同源);
6. **门禁**:里程碑退出标准达成后,向人类总监提交门禁报告再进入下一里程碑。

### 4.3 常见维护任务清单

| 任务 | 步骤 |
| :--- | :--- |
| 新增内建函数 | 06-std.md 定义 → `aether-interp/src/builtins.rs` 实现 + 注册 → `aether-verify/src/typecheck.rs` 加类型签名 → 两端各加测试 |
| 新增语法形式 | 01-syntax.md → `aether-ast` 节点 → `aether-parser` → `aether-verify` + `aether-vm` 编译 → 打印器 → 示例 + round-trip |
| 扩展静态验证域 | 04-contracts.md 支持域表 → `z3bridge.rs` 翻译分支(注意无假阳性承诺) |
| 发新版本 | 冻结 spec 对应章节 → 打 tag(`M1`/`M2`/…)→ 更新 README 状态表 |

### 4.4 v0.1 边界(刻意没有的东西)

无文件/网络 IO、无循环(用递归)、无 import(代码并入模块)、无赋值、无用户级泛型、无字符串索引。每个「缺失」都是设计取舍,不是遗漏;要加,走 4.2 流程 + ADR。

## 5. 一句话总结

人类把需求说成契约,AI 把契约写成代码,验证器证明代码配得上契约——你负责第一句和最后一关。
