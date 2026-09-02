# M2 性能基线(记录,不设硬指标)

> 目标:记录树遍历解释器的初始性能,供 M5 性能后端对比。
> 环境:本机 Windows,release 构建(optimized),wall-clock。

## 结果(M2,release)

| 用例 | Aether | CPython 3.13 | 备注 |
| :--- | :--- | :--- | :--- |
| `fib(25)`(递归) | 381 ms | 54 ms | 朴素树遍历 + 每次调用分配 Env/Rc + 契约检查;CPython 有专用字节码帧 |
| `fold + range 1_000_000`(求和) | 396 ms | — | 每元素一次闭包调用(Env 分配) |

## 结论

- 正确性优先目标达成;**执行速度当前约比 CPython 慢一个数量级**,符合 M2 预期(树遍历、全共享不可变值、每次调用新建词法环境)。
- 已知主要开销(供 M5 参考):①函数调用 Env 分配;②值全部 Rc 克隆;③契约在每次调用时求值;④无 TCO。
- 下一步数据点:M3 完成后重测(契约静态化可跳过部分运行期检查),M5 后端选型以此为对照。

## 复现

```powershell
cargo build --release
# 见 crates/aether-interp/tests/run.rs 同款程序,写入临时 .ae 后:
.\target\release\aether-cli.exe run <bench>.ae
```
