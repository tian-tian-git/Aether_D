# M4 AI 生成回路:基准报告
> 模型:本地 Ollama `gemma3:4b`(temperature 0.2);修补轮数上限 3;每任务每语言独立会话。
> Aether 验证链:parse → typecheck + Z3 静态契约 → 测试运行;Python 验证链:py_compile(隐式)→ 测试运行。反馈均为结构化诊断(非原始堆栈)。

## 指标汇总
| 指标 | Aether | Python |
|---|---|---|
| 一次通过率(first-pass) | 0% | 100% |
| 最终通过率(≤3 轮修补) | 0% | 100% |
| 平均修补轮数 | 3.00 | 1.00 |
| 平均 prompt tokens | 1467 | 109 |
| 平均 completion tokens | 192 | 191 |

## 逐任务明细

| 任务 | Aether 轮/通过 | Python 轮/通过 | Aether tokens | Python tokens |
|---|---|---|---|---|
| fib | 3/✗ | 1/✓ | 1467/192 | 109/191 |
| fact | - | - | - | - |
| gcd | - | - | - | - |
| sum | - | - | - | - |
| max-elem | - | - | - | - |
| filter-even | - | - | - | - |
| map-square | - | - | - | - |
| reverse | - | - | - | - |
| is-prime | - | - | - | - |
| contains | - | - | - | - |
| count | - | - | - | - |
| dot | - | - | - | - |
| sorted-insert | - | - | - | - |
| qsort | - | - | - | - |
| binary-search | - | - | - | - |
| hanoi | - | - | - | - |
| power | - | - | - | - |
| digit-sum | - | - | - | - |
| celsius | - | - | - | - |
| triangle | - | - | - | - |

## 结论(自动生成,待主智能体解读)
