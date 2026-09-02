# 06 标准库(v0.1)

> 状态:**草案**(M2 随解释器实现冻结;冻结前内建函数列表可增删,冻结后变更走 ADR)。
> 所有内建以普通调用形式使用:`(head xs)`、`(+ 1 2)`。
> 契约谓词(`sorted?` 等)由本文件定义,供 `:pre`/`:post` 表达式与用户代码共用。

## 0. 值规范表示(canonical form)

`out`/`err-out` 与打印器(WP1.4)共用同一套规范表示;任何**数据值**(Int/Float/Bool/Str/Vec/Map/Struct/Option/Result/Ast)都有唯一的规范文本,且该文本可被解析器回读(round-trip);`Unit`(`()`)、`Fn`(`<fn name>`/`<lambda>`)为显示形式,不作回读承诺:

| 值 | 规范表示 | 示例 |
| :--- | :--- | :--- |
| `Int` | 十进制(负数带 `-`) | `42`、`-7` |
| `Float` | 最短往返表示(round-trip 保证) | `3.14` |
| `Bool` | `true` / `false` | — |
| `Str` | 带引号 + 转义(`\n` `\t` `\r` `\\` `\"` 及 `\u{...}`) | `"hi\n"` |
| `Vec` | `(vec e1 e2 ...)` | `(vec 1 2 3)` |
| `Map` | `(map-of (k v) ...)`,键值按键排序输出 | `(map-of ("a" 1))` |
| `Struct` | `(Name f1 f2 ...)`(按字段声明序) | `(Point 1.0 2.0)` |
| `Fn` | `<fn name>` / `<lambda>` | — |
| `Option` | `(some x)` / `none` | — |
| `Result` | `(ok x)` / `(err e)` | — |
| `Unit` | `()` | — |
| `Ast` | 其语法的规范 S 表达式 | `(+ 1 2)` |

`out` 对 `Str` 例外:输出**裸字符串**(不加引号);其余值一律规范表示 + 换行。

## 1. 算术(纯函数)

| 内建 | 元数 | 语义 |
| :--- | :--- | :--- |
| `+` | n≥0 | 数值相加;`(+)`=0;Int 按 i64 环绕 |
| `-` | n≥1 | 1 元取负;≥2 元左折叠相减 |
| `*` | n≥0 | 数值相乘;`(*)`=1 |
| `/` | 2 | 数值相除;Int÷Int 为**截断向零**整除,除零 E5xxx |
| `%` | 2 | Int 取余(符号随被除数),除零 E5xxx |
| `abs` | 1 | 绝对值(Int/Float) |
| `min` / `max` | n≥1 | 数值最小/最大 |
| `rand` | 1 | 确定性伪随机(LCG):`(rand s)` = `(s*1103515245+12345) mod 2^31`,i64 环绕语义;纯函数(种子入 → 下一值出),与确定性承诺一致;非线性算术,**不在 Z3 静态验证域**(运行期) |

- 无隐式转换:Int 与 Float 混用 → E5xxx(类型不匹配)。
- Int 溢出按环绕语义(定义行为,非 UB);Float 按 IEEE 754。

## 2. 比较与逻辑(纯函数)

| 内建 | 元数 | 语义 |
| :--- | :--- | :--- |
| `==` / `!=` | 2 | **结构深相等**及其否定;全部类型可用 |
| `<` `<=` `>` `>=` | 2 | 全序比较;仅 Int/Float/Bool/Str 可用,其余类型 E5xxx |
| `and` / `or` | n≥0 | 逻辑与/或;**严格求值**(v0.1 不短路,见 02-semantics §1);`(and)`=true、`(or)`=false |
| `not` | 1 | 逻辑非 |

## 3. 谓词(纯函数)

| 内建 | 元数 | 语义 |
| :--- | :--- | :--- |
| `empty?` | 1 | Vec/Map/Str 是否为空 |
| `finite?` | 1 | Float 是否有限 |
| `sorted?` | 1 | Vec 是否按 `<` 非降序(供契约使用) |
| `permutation?` | 2 | 两个 Vec 元素多重集是否相等(供契约使用) |

## 4. 集合操作(纯函数,不可变:一律返回新值)

| 内建 | 元数 | 语义 |
| :--- | :--- | :--- |
| `len` | 1 | Vec/Map/Str 长度(Str 按码点) |
| `head` / `tail` | 1 | 首元素 / 去掉首元素的新 Vec;空 Vec → E5xxx |
| `get` | 2 | `(get xs i)`:Vec 按 0 起索引(越界 E5xxx);`(get m k)`:Map 按键(缺键 E5xxx) |
| `push` | 2 | `(push xs x)` → 尾部追加后的新 Vec |
| `concat` | n≥0 | Vec 拼接或 Str 拼接(全部同类型);`(concat)`=空 Vec |
| `filter` | 2 | `(filter pred xs)`,pred 为 `(Fn T -> Bool)` |
| `map` | 2 | `(map f xs)` → 逐元素变换的新 Vec |
| `fold` | 3 | `(fold f init xs)` → 左折叠 |
| `range` | 2 | `(range lo hi)` → `[lo, hi)` 整数升序 Vec(lo>hi 为空);规模上限 1_000_000(E5014,资源守卫) |
| `has?` | 2 | Map 是否含键 |
| `keys` | 1 | Map 的键列表(排序后返回,保证确定性) |
| `put` | 3 | `(put m k v)` → 含该键值对的新 Map |

## 5. 字符串与转换(纯函数)

| 内建 | 元数 | 语义 |
| :--- | :--- | :--- |
| `str-len` | 1 | Str 码点数 |
| `int->str` | 1 | Int → Str(十进制) |
| `str->int` | 1 | Str → Int;非十进制整数文本 → E5xxx |

## 6. Option / Result

| 内建 | 元数 | 语义 |
| :--- | :--- | :--- |
| `some` / `none` / `ok` / `err` | 1 / 0 / 1 / 1 | 显式构造 |
| `is-some?` `is-none?` `is-ok?` `is-err?` | 1 | 判别 |
| `unwrap` | 1 | 取出内容;`none`/`err` → E5xxx |

## 7. IO 与效果(效果函数:调用方须在签名带 `!` 标记,见 02-semantics §6)

| 内建 | 元数 | 语义 |
| :--- | :--- | :--- |
| `out` | 1 | stdout 写一行(Str 裸输出,其余规范表示),返回 `()` |
| `err-out` | 1 | 同 `out` 但写 stderr(与 Result 构造器 `err` 区分) |

v0.1 无文件/网络/输入能力。

## 8. 同像性

| 内建 | 元数 | 语义 |
| :--- | :--- | :--- |
| `ast->str` | 1 | Ast 值 → 规范 S 表达式 Str(与打印器同源) |

`eval` 与结构补丁列入 M6(自举)。

## 8.5 图形输出:SVG(文本产物,纯函数)

> SVG 是纯文本,天然属于 Aether 的能力域;Aether 负责**构图计算**(坐标/尺寸/配色),
> 浏览器负责显示。转义在内部完成(即 `svg-text` 的 content 含 `<`/`&`/引号也安全)。

| 内建 | 元数 | 语义 |
| :--- | :--- | :--- |
| `svg-text` | 5 | `(svg-text x y size fill s)` → `<text x='..' y='..' font-size='..' fill='..'>转义后的 s</text>` |
| `svg-circle` | 4 | `(svg-circle cx cy r fill)` → `<circle cx='..' cy='..' r='..' fill='..'/>` |

## 9. 冻结条件(M2)

1. 解释器实现本文件全部内建,且每个内建 ≥1 个行为测试(golden)。
2. `sorted?`/`permutation?` 能被 M3 契约运行时检查直接复用。
3. 冻结后新增内建须 ADR + 同步 examples 与测试。
