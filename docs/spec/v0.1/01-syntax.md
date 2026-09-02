# 01 词法与语法(v0.1)

> 状态:候选冻结稿(待 M1 门禁确认)。冻结后任何变更须走 ADR。

## 1. 设计原则

1. **AST 即语法**:S 表达式,一切皆前缀形式;没有运算符优先级表,没有隐式规则。
2. **全显式**:每个参数、每个 `let` 绑定必须携带类型标注;无隐式类型转换。
3. **最小 Token 集合**:不为人类可读性引入冗余语法糖。

## 2. 词法

### 2.1 Token 类别

| 类别 | 定义 | 示例 |
| :--- | :--- | :--- |
| 空白 | space / tab / CR / LF,仅分隔 token | — |
| 注释 | `;;` 至行尾,忽略 | `;; 说明` |
| 左/右括号 | `(` `)` | — |
| 冒号 | `:`(契约标记前缀) | `:pre` |
| 箭头 | `->`(双字符 token) | — |
| 效果标记 | `!`(显式效果占位) | — |
| 整数字面量 | `-?[0-9]+` 或 `-?0x[0-9a-fA-F]+` | `42`、`-7`、`0xFF` |
| 浮点字面量 | `-?[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?`(整数部分与小数部分均不可省略) | `3.14`、`-0.5`、`1e3` 不合法,写 `1.0e3` |
| 字符串字面量 | `"..."`,转义仅 `\\` `\"` `\n` `\t` `\r` `\u{XXXX}`;字面换行非法 | `"hi\n"` |
| 标识符 | `[A-Za-z_+\-*/<>=!?.][A-Za-z0-9_+\-*/<>=!?.]*`(允许运算符字符与 `-`/`?`/`.`,故 `+`、`finite?`、kebab-case 均为合法标识符) | `qsort`、`<=`、`get-user-name` |

**词法歧义消解**(固定规则,无回溯猜测):

1. `-` 后紧跟数字 → 负数字面量;`0x` 后跟十六进制位 → 十六进制整数。
2. 数字字面量优先于标识符:`1.5` 是浮点数,不是标识符 `1` `.` `5`。
3. `->` 优先于标识符 `-` 与 `>` 的相邻出现。
4. `;;` 优先于标识符(标识符中 `;` 不合法)。
5. 词法错误:`;` 单独出现(E1001,提示使用 `;;`);未闭合字符串(E1002);非法字符(E1003)。

### 2.2 保留字

作为**特殊形式头**的标识符:`module` `fn` `let` `struct` `if` `block` `vec` `map` `quote`。
作为**字面量**的标识符:`true` `false`。
其余标识符一律按函数/变量名处理——**没有关键字污染**:`(let fn Int 1)` 合法(但不建议,见 02 命名约定)。

## 3. 语法(BNF)

```bnf
program   := form*
form      := (module IDENT form*)
          |  (fn IDENT param* "->" TYPE fx? contract* body)
          |  (struct IDENT field* invariant*)
          |  (let IDENT TYPE expr)
          |  expr

param     := (IDENT TYPE)
field     := (IDENT TYPE)
contract  := ":pre" expr | ":post" expr      ;; fn 仅允许 :pre / :post
invariant := ":invariant" expr          ;; struct 仅允许 :invariant
fx        := "!"

TYPE      := "()" | "Int" | "Float" | "Bool" | "Str" | "Ast" | "Unit"
          |  "(Vec" TYPE ")"
          |  "(Map" TYPE TYPE ")"
          |  "(Option" TYPE ")"
          |  "(Result" TYPE TYPE ")"
          |  "(Fn" "(" TYPE* ")" "->" TYPE ")"
          |  IDENT                            ;; 结构体名

expr      := INT | FLOAT | STR | true | false
          |  IDENT                             ;; 变量引用
          |  (quote expr)                      ;; 同像:返回 Ast 值
          |  (if expr expr expr)               ;; else 必填,不提供省略形式
          |  (block expr*)                     ;; 值 = 末表达式;空 block = ()
          |  (vec expr*)                       ;; 向量字面量
          |  (map (expr expr)*)                ;; 映射字面量,键值成对
          |  (let IDENT TYPE expr)             ;; let 作为表达式:值为初值,绑定作用于所在作用域后续(02 §3)
          |  (fn IDENT? param* "->" TYPE fx? contract* body)  ;; 具名/匿名函数(匿名=省略 IDENT)
          |  (IDENT expr*)                     ;; 调用 / 结构体构造 / 内建函数
body      := expr
```

### 3.1 各形式要点

| 形式 | 元数 | 说明 |
| :--- | :--- | :--- |
| `(module name form*)` | ≥1 | 文件名应等于模块名(`sort.ae` → `(module sort ...)`);约定而非强制,解析器不校验;嵌套 `module` 在 v0.1 非法(E2xxx),为未来命名空间预留 |
| `(fn name params -> type [fx] [contracts] body)` | 见文法 | 具名函数;fx `!` 表示显式效果标记(首版解析保留、不检查);契约顺序任意但建议 `:pre` → `:post` |
| `(fn params -> type [fx] [contracts] body)` | 匿名 | 首元素为 `(` 即匿名函数,作为值参与调用/传参 |
| `(struct Name fields [:invariant e]*)` | ≥1 | 定义结构体与构造器;`:invariant` 可多个 |
| `(let IDENT TYPE expr)` | 3 | **既是顶层项也是表达式**:表达式值为初值;绑定从声明点起在该作用域后续可见(见 02 §3) |
| `(if cond then else)` | 3 | else **必填**;无单臂形式(确定性优先) |
| `(block expr*)` | 任意 | 顺序求值,值 = 末表达式;空 block 的值为 `()` |
| `(vec expr*)` | 任意 | 向量字面量 |
| `(map (k v)*)` | 偶数 | 每对键值为一个双元素形式 |
| `(quote expr)` | 1 | 不求值 expr,返回其 Ast 值(同像性的第一个落地能力) |
| `(IDENT expr*)` | 任意 | 调用:实参从左到右严格求值 |

### 3.2 显式禁止(v0.1 不做)

- 赋值/原地更新:`set!`、`=` 绑定语法一律不存在。
- 中缀运算符、运算符优先级:`a + b * c` 非法,必须 `(+ a (* b c))`。
- 隐式类型转换、类型省略、默认参数、可变参数。
- 循环语法(`while`/`for`):v0.1 以递归表达(M2 起评估是否引入)。

## 4. 解析行为约定

1. 解析器是**手写递归下降**,不引入解析器生成器。
2. 首个语法错误即停,报告 E2xxx 诊断(含 span 与修复建议),见 05-diagnostics。
3. `examples/` 全部程序是解析验收集;新增语法形态必须先入示例集再进实现。
