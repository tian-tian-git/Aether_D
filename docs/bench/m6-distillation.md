# M6 静态蒸馏实验报告(WP6.3)

> 实验定义:把 **Z3 验证过的 Aether 标准库**(`lib/aether-std.ae`,19 函数、静态验证零违反)作为「已验证片段语料」注入系统提示,重跑 M4 基准中全部失败的 fold 类任务,对比原提示下的成绩。

## 结果(gemma3:4b,8 个 M4 失败任务)

| 任务 | 原提示(M4) | +蒸馏语料(M6) | 结论 |
| :--- | :--- | :--- | :--- |
| filter-even | ✗ 3 轮 | ✅ 一次通过 | 修复 |
| reverse | ✗ 3 轮 | ✅ 一次通过 | 修复 |
| contains | ✗ 3 轮 | ✅ 一次通过 | 修复 |
| count | ✗ 3 轮 | ✅ 一次通过 | 修复 |
| dot | ✗ 3 轮 | ✅ 一次通过 | 修复 |
| max-elem | ✗ 3 轮 | ✗ | 仍失败 |
| map-square | ✗ 3 轮 | ✗ | 仍失败 |
| is-prime | ✗ 3 轮 | ✗ | 仍失败 |
| **合计** | **0/8** | **5/8(62.5%)** | **+62.5pp** |

## 解读

1. **静态蒸馏直接命中 M4 诊断的根因**。M4 结论:「示例同构 → 成功,结构组合 → 幻觉」。注入的语料正是 fold/filter/map/concat 的**已验证组合模式**,模型从语料中直接复用结构,幻觉消失——filter-even、contains、count、dot 全部一次通过。
2. **验证是蒸馏的前提**。语料来自 Z3 静态验证零违反的标准库——「蒸馏」蒸馏的是**已证明正确**的知识,不是任意代码。这是 Aether 相对普通代码库的结构性优势:验证器是语料质量的守门人。
3. **剩余 3 个失败**集中在需要「谓词构造 + 契约组合」的任务(is-prime 的 fold-with-or 惯用法、max-elem 的 head/tail fold)。语料中没有它们的同构模式——补强方向明确(在语料中加入这些模式,预计同样可修复;留作后续迭代)。
4. **token 成本**:注入语料使 prompt tokens 从 ~650 增至 ~2000,但仍低于 M4 失败时的多轮修补总量(~2200+/任务),且一次通过率从 0 提升至 62.5%——单位正确性的成本显著下降。

## M6 退出标准对照

| 退出标准 | 达成 |
| :--- | :--- |
| 自举 demo 可复现 | ✅ `lib/aether-std.ae`(19 函数 Aether 实现 + 6 项集成测试 + `aether run` demo) |
| 热更新 demo 可复现 | ✅ `crates/aether-vm/examples/graph_patch.rs`(NodeId 锚点节点替换,42→43) |
| 静态蒸馏实验报告 | ✅ 本文件(+62.5pp 量化改善) |

## 复现

```powershell
# 图补丁 demo
cargo run -p aether-vm --example graph_patch
# 蒸馏实验(本地 Ollama gemma3:4b,约 30 分钟)
python tools/harness/harness.py --tasks filter-even,map-square,contains,count,reverse,max-elem,dot,is-prime --corpus lib/aether-std.ae --report-out docs/bench/m4-distill.md
```
