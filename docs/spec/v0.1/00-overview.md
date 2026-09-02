# Aether 语言规范 v0.1(草案)

> **状态:草案** —— 由 M1(WP1.1)细化并冻结;冻结后任何行为变更须走 ADR。
> 规范与实现同步版本化:实现侧 `aether-ast::SPEC_VERSION` 须与本目录一致。

## 目录(冻结时补齐)

| 文件 | 内容 | 状态 |
| :--- | :--- | :--- |
| `00-overview.md` | 总览与设计原则(本文件) | 草案 |
| `01-syntax.md` | 词法与语法(BNF) | M1 冻结 |
| `02-semantics.md` | 求值模型与作用域 | M1 冻结 |
| `03-types.md` | 类型系统 | M3 冻结 |
| `04-contracts.md` | 契约语义与验证 | M3 冻结 |
| `05-diagnostics.md` | 结构化诊断规范 | M1 冻结 |
| `06-std.md` | 标准库 | M2 起持续 |

## 语法草图(S 表达式,草案示例)

```clojure
;; 行注释以 ;; 开头

;; 模块:文件名即模块名,module 声明可选
(module hello
  (fn main () -> ()
    (out "Hello, Aether!")))

;; 函数: (fn 名 (参数 类型)* -> 返回类型 契约* 体)
;; 契约: :pre / :post / :invariant 块,result 指代返回值
(fn add (a Int) (b Int) -> Int
  :post (== result (+ a b))
  (+ a b))

;; 不可变绑定: (let 名 类型 初值)
(let answer Int 42)

;; 分支: (if 条件 真分支 假分支)
;; 块: (block 语句* 末表达式)
;; 结构体:
(struct Point (x Float) (y Float))
```

## 语义原则(v0.1 不可谈判项)

1. **一切皆表达式**;块的值 = 末表达式。
2. **不可变默认**:`let` 只绑定一次;无隐式类型转换;无 `null`(用 `Option`/`Result`)。
3. **无歧义**:前缀调用,无运算符优先级;无隐式魔法。
4. **显式效果**:纯函数默认;IO/状态须显式标记(最小实现:内建 `out` 等效果函数 + 调用方声明)。
5. **契约一等化**:`pre`/`post`/`invariant` 是语法,违反即结构化诊断,不是注释。
6. **同像性方向**:AST 可被反射读取(首版);结构级补丁(M6)。

## 类型系统(v0.1 子集)

`Int / Float / Bool / Str / Option[T] / Result[T, E] / Vec[T] / Map[K, V] / Struct / Fn`;
静态检查在 M3 启用,M2 解释器仅做运行期检查。

## 示例程序

见仓库 `examples/`(`hello.ae` 等),M1 起作为解析与执行的验收集。

## 命名与风格(内部约定)

- 语法关键字:小写 ASCII(`fn`、`let`、`struct`、`if`、`block`、`module`、`true`、`false`)。
- 类型名:首字母大写(`Int`、`Vec`)。
- 变量/函数名:kebab-case(`get-user-name`);不强制,但示例与 std 遵守。
