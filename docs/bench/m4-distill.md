# M4 AI 生成回路:基准报告
> 模型:本地 Ollama `gemma3:4b`(temperature 0.2);修补轮数上限 3;每任务每语言独立会话。
> Aether 验证链:parse → typecheck + Z3 静态契约 → 测试运行;Python 验证链:py_compile(隐式)→ 测试运行。反馈均为结构化诊断(非原始堆栈)。

## 指标汇总
| 指标 | Aether | Python |
|---|---|---|
| 一次通过率(first-pass) | 62% | 100% |
| 最终通过率(≤3 轮修补) | 62% | 100% |
| 平均修补轮数 | 1.75 | 1.00 |
| 平均 prompt tokens | 3606 | 88 |
| 平均 completion tokens | 116 | 83 |

## 逐任务明细

| 任务 | Aether 轮/通过(失败因) | Python 轮/通过(失败因) | Aether tokens | Python tokens |
|---|---|---|---|---|
| fib | - | - | - | - |
| fact | - | - | - | - |
| gcd | - | - | - | - |
| sum | - | - | - | - |
| max-elem | 3/✗(other) | 1/✓ | 6178/165 | 100/74 |
| filter-even | 1/✓ | 1/✓ | 2010/47 | 90/80 |
| map-square | 3/✗(syntax) | 1/✓ | 6315/147 | 85/43 |
| reverse | 1/✓ | 1/✓ | 2000/55 | 80/38 |
| is-prime | 3/✗(syntax) | 1/✓ | 6332/328 | 101/120 |
| contains | 1/✓ | 1/✓ | 2003/49 | 83/91 |
| count | 1/✓ | 1/✓ | 2005/56 | 85/95 |
| dot | 1/✓ | 1/✓ | 2004/77 | 84/124 |
| sorted-insert | - | - | - | - |
| qsort | - | - | - | - |
| binary-search | - | - | - | - |
| hanoi | - | - | - | - |
| power | - | - | - | - |
| digit-sum | - | - | - | - |
| celsius | - | - | - | - |
| triangle | - | - | - | - |

### Aether 失败原因分布

- syntax: 2
- other: 1

## 结论(自动生成,待主智能体解读)
