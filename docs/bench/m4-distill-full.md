# M4 AI 生成回路:基准报告
> 模型:本地 Ollama `gemma3:4b`(temperature 0.2);修补轮数上限 3;每任务每语言独立会话。
> Aether 验证链:parse → typecheck + Z3 静态契约 → 测试运行;Python 验证链:py_compile(隐式)→ 测试运行。反馈均为结构化诊断(非原始堆栈)。

## 指标汇总
| 指标 | Aether | Python |
|---|---|---|
| 一次通过率(first-pass) | 55% | 0% |
| 最终通过率(≤3 轮修补) | 55% | 0% |
| 平均修补轮数 | 1.90 | 0.00 |
| 平均 prompt tokens | 3944 | 0 |
| 平均 completion tokens | 132 | 0 |

## 逐任务明细

| 任务 | Aether 轮/通过(失败因) | Python 轮/通过(失败因) | Aether tokens | Python tokens |
|---|---|---|---|---|
| fib | 1/✓ | - | 2029/51 | - |
| fact | 1/✓ | - | 2013/47 | - |
| gcd | 3/✗(type) | - | 6369/160 | - |
| sum | 1/✓ | - | 2001/42 | - |
| max-elem | 1/✓ | - | 2020/63 | - |
| filter-even | 1/✓ | - | 2010/47 | - |
| map-square | 3/✗(syntax) | - | 6315/147 | - |
| reverse | 1/✓ | - | 2000/55 | - |
| is-prime | 3/✗(syntax) | - | 6332/286 | - |
| contains | 1/✓ | - | 2003/49 | - |
| count | 1/✓ | - | 2005/56 | - |
| dot | 1/✓ | - | 2004/77 | - |
| sorted-insert | 1/✓ | - | 2014/88 | - |
| qsort | 3/✗(syntax) | - | 6346/342 | - |
| binary-search | 3/✗(type) | - | 6271/346 | - |
| hanoi | 3/✗(other) | - | 6181/156 | - |
| power | 1/✓ | - | 2020/54 | - |
| digit-sum | 3/✗(type) | - | 6338/178 | - |
| celsius | 3/✗(type) | - | 6352/142 | - |
| triangle | 3/✗(syntax) | - | 6249/262 | - |

### Aether 失败原因分布

- type: 4
- syntax: 4
- other: 1

## 结论(自动生成,待主智能体解读)
