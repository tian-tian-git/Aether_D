# Aether 语言规范 v0.1

> **状态:候选冻结稿**(WP1.1 完成;待 M1 门禁由人类总监确认冻结)。冻结后任何行为变更须走 ADR。
> 规范与实现同步版本化:实现侧 `aether-ast::SPEC_VERSION` 与本文档版本一致。

## 目录

| 文件 | 内容 | 状态 |
| :--- | :--- | :--- |
| `00-overview.md` | 总览与设计原则(本文件) | ✅ 候选冻结 |
| `01-syntax.md` | 词法与语法(BNF) | ✅ 候选冻结 |
| `02-semantics.md` | 语义与求值模型 | ✅ 候选冻结 |
| `05-diagnostics.md` | 结构化诊断规范 | ✅ 候选冻结 |
| `03-types.md` | 类型系统 | M3 冻结 |
| `04-contracts.md` | 契约语义与验证 | M3 冻结 |
| `06-std.md` | 标准库 | 草案(M2 冻结) |

## 语法一瞥(v0.1 定稿形态)

```clojure
;; 行注释

;; 模块 + 入口(显式效果标记 ! 表示 main 有 IO)
(module hello
  (fn main () -> () !
    (out "Hello, Aether!")))

;; 函数 + 契约(result 指代返回值;谓词以 ? 结尾)
(fn qsort (xs (Vec Int)) -> (Vec Int)
  :post (sorted? result)
  (if (empty? xs)
      xs
      (concat (qsort (filter (fn (x Int) -> Bool (< x (head xs))) xs))
              (vec (head xs))
              (qsort (filter (fn (x Int) -> Bool (> x (head xs))) xs)))))

;; 不可变绑定(类型必填)
(let answer Int 42)

;; 结构体 + 不变量
(struct Point (x Float) (y Float)
  :invariant (and (finite? x) (finite? y)))

;; 同像性
(quote (+ 1 2))   ;; => Ast 值
```

## 语义原则(v0.1 不可谈判项)

1. **一切皆表达式**;块的值 = 末表达式;`if` 三臂必填。
2. **不可变默认**:`let` 只绑定一次;无赋值语法;无隐式类型转换;无 `null`(用 `Option`/`Result`)。
3. **无歧义**:前缀调用,无运算符优先级;无隐式魔法;每个绑定显式标注类型。
4. **显式效果**:纯函数默认;IO 内建仅 `out`/`err`;`!` 标记为首版占位(M4+ 实施检查)。
5. **契约一等化**:`pre`/`post`/`invariant` 是语法,违反即结构化诊断。
6. **同像性方向**:`quote` 返回 Ast 值(首版);`eval` 与结构补丁在 M6。
7. **零未定义行为**:所有操作要么有定义,要么抛结构化错误。

## 类型系统(v0.1 语法)

`()`=Unit、`Int`、`Float`、`Bool`、`Str`、`Ast`、`(Vec T)`、`(Map K V)`、`(Option T)`、`(Result T E)`、`(Fn (T*) -> T)`、结构体名。
静态检查在 M3 启用,M2 解释器仅做运行期检查。

## 示例程序

见仓库 `examples/`(`hello.ae`、`fib.ae`、`sort.ae`、`point.ae`),M1 起作为解析与执行的验收集。
