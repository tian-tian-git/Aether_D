# ADR-0002:M5 性能后端选型(已定稿)

- **状态**:已定稿(M5 实施完成)
- **决策**:字节码 VM(方案 A)——已实现为 `crates/aether-vm` 并设为 `aether run` 默认后端(`--interp` 回退)。
- **结果**:见 `docs/bench/m5-bench.md`(fib 2.3×、fold 1.9× CPython,达成「同量级或更优」)。

## 实施要点(记录备查)

1. 单遍编译器:栈槽分配 + Lua 式上值链(Local/ParentUpvalue/SelfRef);
2. 契约编译为内联字节码(:pre 入口 / :post 返回前 / :invariant 构造处,JumpIfTrue 跳过 Raise*);
3. 高阶内建 filter/map/fold 编译为原生 HOF 指令(谓词闭包 VM 内联执行,避免解释器回退——初版回退实测慢 15 倍);
4. 内建查表:编译期内建名表 + 函数指针表对齐,运行时零 HashMap;
5. 跳转补丁偏移以「下一条指令」为基准(修过一个 2 字节错位 bug);
6. 调用时按 nslots 预留槽区(StoreLocal 需要已存在的槽)。

## 后续候选(M5.5+,按需评估)

- 寄存器式直接线程化 + 值内联(进一步缩小与 CPython 的 2× 差距);
- Cranelift 原生后端(ADR-0002 分析中的方案 B,windows-gnu 可行性 spike 先行)。

