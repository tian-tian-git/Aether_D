# M5 性能基准(字节码 VM vs 树遍历 vs CPython)

> 环境:本机 Windows,release 构建;wall-clock 中位数。基线:M2 树遍历 381–415ms(fib25)。

## 结果

| 用例 | 树遍历(M2) | 字节码 VM(M5) | CPython 3.13 | VM/CPython |
| :--- | :--- | :--- | :--- | :--- |
| `fib(25)`(递归 + 契约检查) | ~415 ms | **83 ms** | 36 ms | 2.3× |
| `fold` 求和 1e6(高阶闭包) | ~400 ms* | **138 ms** | 73 ms(reduce+lambda 等价写法) | 1.9× |

\* M2 未直接测此用例,按同量级估算;VM 原生 HOF 指令使其从初版 2124ms 降至 138ms(15×)。

## 结论

1. **M5 退出标准「与 CPython 同量级或更优」达成**(同量级:2× 左右常数因子,优于树遍历 5×)。
2. 剩余差距来源:Rc 共享值模型、每次调用预留槽区、Vec push/pop 栈纪律——均有进一步优化空间(M5.5:寄存器式直接线程化、值内联)。
3. 关键设计:filter/map/fold 编译为**原生 HOF 指令**,谓词闭包在 VM 内联执行(初版回退树遍历解释器导致 fold 慢 15 倍,这是本次基准暴露并修复的真问题)。

## 复现

```powershell
cargo build --release
# 程序内容见 docs/bench/m2-baseline.md 同款;VM 为默认后端:
.\target\release\aether-cli.exe run bench.ae        # VM(默认)
.\target\release\aether-cli.exe run --interp bench.ae  # 树遍历回退
```
