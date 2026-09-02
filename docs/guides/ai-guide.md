# Aether AI 指南:使用与维护

> 读者:AI(LLM 与智能体)。目标:高效生成**正确**的 Aether 代码,并参与维护这门语言。
> 语言权威定义在 [spec v0.1](../spec/v0.1/00-overview.md);本指南是可操作的速记与纪律。
> 给人类看的对应文档:[human-guide.md](human-guide.md)。

## 0. 铁律(生成任何代码前先记住)

1. **契约先行**:人类的需求要翻译成 `:pre`/`:post` 契约,再写实现。契约是验收标准,不是注释。
2. **全显式**:每个参数、每个 `let` 绑定必须带类型;无隐式转换;`Int` 与 `Float` 不混用。
3. **无循环**:迭代一律用递归或 `fold`/`map`/`filter`。
4. **函数体是单表达式**:多个语句必须包 `(block ...)`。
5. **产出代码**:被要求时只输出代码本身(不带 markdown 围栏、不带解释),除非对方要求解释。
6. **验证优先**:生成后立即用 `aether check`(或 `parse --json`)验证;失败就按诊断修补,不要凭感觉重写。

## 1. 语法速记(完整、可复制进系统提示)

```text
;; 注释
(module name ...)                                    ;; 模块(每个文件最多一个,置于顶层)

(fn name (p Type) ... -> RetType [:pre e] [:post e] body)   ;; 具名函数
(fn (p Type) ... -> RetType body)                           ;; 匿名函数(值)
;; :pre 在入口检查;:post 在返回前检查,用 result 指代返回值;契约必须是 Bool 表达式

(let name Type value)         ;; 不可变绑定:从声明点起作用到所在作用域结束;let 也是表达式(值为初值)
(block e1 e2 ...)             ;; 顺序求值,值为末表达式;空 block 值为 ()
(if cond then else)           ;; 三臂必填;cond 必须 Bool
(vec 1 2 3)                   ;; 向量字面量
(map-of ("k" v) ...)          ;; 映射字面量(注意:变换函数是 map,字面量是 map-of)
(quote expr)                  ;; 返回 Ast 值(同像性);(ast->str a) 转为规范源码
(. obj "field")               ;; 字段访问,字段名是字符串字面量
none                          ;; 等价于 (none);some/ok/err 用 (some x) (ok x) (err e)

;; 类型
() Unit | Int | Float | Bool | Str | Ast
(Vec T) (Map K V) (Option T) (Result T E) (Fn (T*) -> R) | 结构体名
```

## 2. 内建函数全表(只许用这些 + 自定义函数)

| 类 | 内建(签名要点) |
| :--- | :--- |
| 算术 | `+ - *`(n 元,全 Int 或全 Float)`/ %`(2 元;Int 截断除,除零 → E5002)`abs min max sqrt`(Float→Float) |
| 比较 | `== !=`(结构深相等,任意类型)`< <= > >=`(Int/Float/Bool/Str) |
| 逻辑 | `and or`(n 元,**严格求值**不短路)`not` |
| 谓词 | `empty? finite? sorted? permutation?` |
| 集合 | `len head tail get push concat filter map fold range(lo,hi)`;`get xs i` 0 起、越界 E5004 |
| Map | `has? keys put`;`get m k` 缺键 E5004 |
| 字符串 | `str-len`(码点)`int->str str->int`(非法数字串 E5010);拼接用 `concat` |
| Option/Result | `some none ok err is-some? is-none? is-ok? is-err? unwrap` |
| IO/效果 | `out`(Str 裸输出,其余规范表示)`err-out`;两者返回 `()`,调用它们的函数应标 `!` |
| 同像 | `ast->str` |

`map`/`filter`/`fold` 是**变换函数**,不是字面量;高阶签名:`(filter (Fn (T) -> Bool) (Vec T)) -> (Vec T)`、`(fold (Fn (A T) -> A) A (Vec T)) -> A`。

## 3. 验证回路(你的生成-修补闭环)

```powershell
aether parse file.ae --json   # → {"ok":true} 或单条诊断
aether check file.ae --json   # → {"ok":true,"verify":"verified"} 或 {"diagnostics":[...]}
aether run  file.ae           # 执行(exit code = main 返回的 Int)
```

**诊断 JSON 字段**:`node_id`(AST 节点锚点)、`severity`、`code`(稳定错误码,**优先按它分类修补**)、`message`、`span{start,end}`(offset 字节/line/col)、`hints[]`(修复建议,至少一条)。

**错误码区间**:

| 区间 | 含义 | 修补策略 |
| :--- | :--- | :--- |
| E1xxx | 词法(非法字符/未闭合字符串/数字格式) | 对照 1 节语法改字面量 |
| E2xxx | 解析(括号/元数/形式位置) | 检查括号平衡;`if` 三臂;函数体单表达式 |
| E3xxx | 类型(不匹配/未定义/元数) | 对照签名表与上下文类型 |
| E4xxx | 契约(静态证伪,**含反例输入**) | 契约太强或实现没守住约定——改实现或修正契约 |
| E5xxx | 运行期(除零/越界/类型) | 加守卫或契约 |
| E6001 | Z3 后端不可用(静态验证跳过,不影响运行) | 忽略,继续 |

**修补纪律**:收到诊断后,(1) 保留原始任务要求;(2) 只改诊断指向的问题;(3) 输出**完整修正代码**;(4) 重验证。有 `hints` 时优先采纳 hints。

## 4. 契约写作规范(写给 AI)

### 4.1 写什么

- 边界约束:`:pre (and (>= i 0) (< i (len xs)))`——静态可证伪,附反例;
- 输出性质:`:post (sorted? result)`、`:post (== (len result) (len xs))`——长度/算术类可静态证,谓词类运行期兜底;
- 结构体不变量:`:invariant (and (finite? x) (>= x 0))`。

### 4.2 反模式(会害了自己)

- **契约比实现强**:`gcd` 写 `:pre (> b 0)` 但递归传 0 —— 验证器会拒绝你自己的调用;
- 契约里用不存在的变量、非 Bool 表达式(E3007);
- 把 `:post` 的 `result` 当成参数名写进 `:pre`。

### 4.3 静态可证域(重要边界)

Z3 静态验证覆盖:`Int`/`Bool`/`(Vec Int)` + 算术/比较/布尔/`get`/`len`/`push`/`head`/`vec`/`range`/`if`/`let`。`Float`/`Str`/`Map`/`Option`/结构体/`sorted?` 等**静默回退运行期检查**——不要假设它们被静态证明,也不要因它们未证明而改写可运行代码。

## 5. 惯用法(来自 Z3 验证过的 `lib/aether-std.ae`)

```clojure
;; 聚合:fold + 匿名函数
(fn sum (xs (Vec Int)) -> Int
  (fold (fn (acc Int) (x Int) -> Int (+ acc x)) 0 xs))

;; 条件计数:if 作表达式
(fn count (xs (Vec Int)) (v Int) -> Int
  (fold (fn (acc Int) (x Int) -> Int (if (== x v) (+ acc 1) acc)) 0 xs))

;; 谓词组合:any?
(fn any? (xs (Vec Int)) (pred (Fn (Int) -> Bool)) -> Bool
  (fold (fn (acc Bool) (x Int) -> Bool (or acc (pred x))) false xs))

;; 递归替代循环(带守卫契约)
(fn take (xs (Vec Int)) (n Int) -> (Vec Int)
  :pre (>= n 0)
  (if (or (== n 0) (empty? xs))
      (vec)
      (concat (vec (head xs)) (take (tail xs) (- n 1)))))

;; 局部具名函数(可递归)
(block
  (let g (Fn (Int) -> Int) (fn g (n Int) -> Int (if (<= n 1) 1 (* n (g (- n 1))))))
  (g 5))
```

## 6. 已知陷阱清单(逐条背下来)

1. 括号必须精确平衡——解析器对模块后残留内容直接报 E2001,不会静默丢弃;
2. 函数体多语句不包 `(block ...)` → 解析错误;
3. `map`(变换函数)≠ `map-of`(字面量);`err`(Result 构造)≠ `err-out`(stderr 输出);
4. 字段访问是 `(. obj "field")`,字段名**字符串**;裸标识符会被当变量求值;
5. `main` 返回 `Unit` 或 `Int`,参数 0 个或 1 个 `(Vec Str)`;
6. 裸 `none` 合法(等价 `(none)`);`true`/`false` 是字面量;
7. 没有中缀、没有 `[]`、没有赋值、没有循环、没有 `null`;
8. `and`/`or` 不短路(两边都会被求值)。

## 7. 维护语言本身(AI 版工作流)

1. **开工先读** [AGENTS.md](../../AGENTS.md) 与本指南,认领 [roadmap](../../roadmap.md) 工作包;
2. **文件边界即任务边界**:不同智能体不得同时编辑同一文件;
3. **crate 地图**:`aether-ast`(数据模型)→ `aether-diagnostic`/`aether-parser` → `aether-verify`(类型+Z3)/`aether-interp`(树遍历)/`aether-vm`(字节码)→ `aether-cli`;依赖单向;
4. **变更顺序**:先改 `docs/spec/v0.1/`,再改实现,同步提交;架构取舍写 ADR;新错误码先登记 `05-diagnostics.md`;
5. **双后端纪律**:语义变更必须同时过 `aether-interp` 与 `aether-vm` 测试(`cargo test`,18 套件 144 项,全部绿才能交付);
6. **常见任务**:
   - 加内建:06-std.md → `builtins.rs` 实现+注册 → `typecheck.rs` 加签名 → 两端测试;
   - 加语法:01-syntax.md → ast 节点 → parser → verify+vm 编译 → 打印器 → 示例+round-trip;
   - 扩展静态验证:04-contracts 支持域表 → `z3bridge.rs` 翻译分支,**保持无假阳性承诺**;
7. **提交规范**:Conventional Commits(`feat(scope): ...`),scope 用 crate 名或里程碑号。

## 8. 蒸馏工作流(把已验证知识变回你的能力)

1. **语料 = 验证过的片段**:只把「parse + check + Z3 零违反」的代码放进语料库(`lib/aether-std.ae` 是现成范本);
2. **注入**:`python tools/harness/harness.py --corpus lib/aether-std.ae`——实测让 fold 类任务一次通过率 0 → 62.5%;
3. **闭环**:生成的代码验证通过后,沉淀回语料,逐步扩展你可直接复用的模式集。

## 9. 自检清单(每次交付前)

- [ ] `aether check` 全绿(类型 + 静态契约无违反)
- [ ] `aether run` 通过用例
- [ ] 所有契约确实表达了需求(不是摆设)
- [ ] 无第 6 节陷阱
- [ ] 若要进语料:确认 Z3 静态验证零违反
