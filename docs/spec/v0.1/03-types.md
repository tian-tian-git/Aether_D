# 03 类型系统(v0.1)

> 状态:草案(M3 冻结)。类型语法见 01-syntax.md TYPE 规则。

## 1. 静态类型 v0.1 子集

`()`=Unit、`Int`、`Float`、`Bool`、`Str`、`Ast`、`(Vec T)`、`(Map K V)`、`(Option T)`、`(Result T E)`、`(Fn (T*) -> T)`、结构体名(`Named`)。

## 2. 核心规则

1. **全显式标注**:参数、`let`、返回类型必须显式标注;检查器以**检查**为主,仅在以下位置做**类型推演**(内部类型变量统一):
   - `(vec ...)` 元素类型、`(map-of ...)` 键值类型、`if` 两分支统一、`none` 的元素类型、内建泛型签名实例化。
2. **用户函数单态**:v0.1 无用户级泛型/子类型/重载;内建函数拥有多态签名(见 §4),调用点用类型变量实例化。
3. **无隐式转换**:`Int` 与 `Float` 不混用(与运行时一致);数值字面量按词法定型。
4. **结构体**:构造器按字段类型逐一检查;字段访问 `(. obj "field")` 按结构体定义取字段类型;`field` 必须为字符串字面量(静态可查)。
5. **契约**:`:pre`/`:post`/`:invariant` 表达式静态类型必须为 `Bool`;`:post` 中 `result` 的类型 = 声明返回类型。
6. **入口**:`main` 返回类型必须为 `Unit` 或 `Int`(静态检查 E3008);`main` 参数为 0 或 1 个(1 个时类型 `(Vec Str)`)。
7. **可见性**:函数体可引用模块级全部 `let` 常量(静态按全模块可见检查);模块级表达式按声明序检查(运行期若引用尚未求值的 `let` → E5001 兜底)。

## 3. 检查时机与错误

- `aether check <file>` 独立检查;`aether run`/`repl` 在解析后、执行前自动检查。
- 类型错误码 E3xxx:类型不匹配(E3001)、重复绑定(E3002)、未定义变量/函数(E3003)、未知类型(E3004)、元数不匹配(E3005)、未知字段(E3006)、条件/契约非 Bool(E3007)、main 返回类型非法(E3008)。注册于 05-diagnostics.md。
- 静态检查是**提前报告**,运行期兜底检查(E5xxx)仍保留:两套检查共享同一份类型规则。

## 4. 内建多态签名(类型变量实例化)

| 内建 | 签名(T/U/A/K/V 为类型变量) |
| :--- | :--- |
| `+` `-` `*` `min` `max` | 全 `Int` → `Int`;全 `Float` → `Float`(≥1 元;`+`/`*` ≥0) |
| `/` `%` | `Int Int -> Int`;`Float Float -> Float`(`%` 仅 Int) |
| `abs` | `Int -> Int`;`Float -> Float` |
| `sqrt` `finite?` | `Float -> Float`;`Float -> Bool` |
| `==` `!=` | `T T -> Bool` |
| `<` `<=` `>` `>=` | `T T -> Bool`,T ∈ {Int, Float, Bool, Str} |
| `and` `or` `not` | `Bool* -> Bool`;`Bool -> Bool` |
| `empty?` | `(Vec T) -> Bool`;`(Map K V) -> Bool`;`Str -> Bool` |
| `sorted?` | `(Vec T) -> Bool` |
| `permutation?` | `(Vec T) (Vec T) -> Bool` |
| `len` | `(Vec T) -> Int`;`(Map K V) -> Int`;`Str -> Int` |
| `head` `tail` | `(Vec T) -> T`;`(Vec T) -> (Vec T)` |
| `get` | `(Vec T) Int -> T`;`(Map K V) K -> V` |
| `push` | `(Vec T) T -> (Vec T)` |
| `concat` | `(Vec T)* -> (Vec T)`;`Str* -> Str` |
| `filter` | `(Fn (T) -> Bool) (Vec T) -> (Vec T)` |
| `map` | `(Fn (T) -> U) (Vec T) -> (Vec U)` |
| `fold` | `(Fn (A T) -> A) A (Vec T) -> A` |
| `range` | `Int Int -> (Vec Int)` |
| `has?` | `(Map K V) K -> Bool` |
| `keys` | `(Map K V) -> (Vec K)` |
| `put` | `(Map K V) K V -> (Map K V)` |
| `str-len` | `Str -> Int` |
| `int->str` `str->int` | `Int -> Str`;`Str -> Int` |
| `some` | `T -> (Option T)` |
| `none` | `-> (Option T)` |
| `ok` | `T -> (Result T E)` |
| `err` | `E -> (Result T E)` |
| `is-some?` `is-none?` | `(Option T) -> Bool` |
| `is-ok?` `is-err?` | `(Result T E) -> Bool` |
| `unwrap` | `(Option T) -> T`;`(Result T E) -> T` |
| `.` | `Named S Str -> 字段类型`(字段须为字面量) |
| `out` `err-out` | `T -> ()` |
| `ast->str` | `Ast -> Str` |

## 5. v0.1 明确不做

用户级泛型、类型别名、子类型、联合类型、效果检查、类型推断(不标注自动推断)——列入后续版本议题。
