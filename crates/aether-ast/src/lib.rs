//! Aether 抽象语法树(AST)定义。
//!
//! 规范:docs/spec/v0.1/01-syntax.md。
//!
//! ## API 契约
//! 本 crate 的全部公开类型是下游 `aether-parser` / `aether-diagnostic` /
//! `aether-interp` / `aether-verify` 的公共接口契约,未经 ADR 不得更改字段名、
//! 类型与可见性。变更时须同步 docs/spec 与全部下游 crate。
//!
//! 节点编号 `NodeId` 由解析器按模块内全局自增分配;诊断、指标与(远期)图补丁
//! 均以它为锚点。

/// 语言名与规范版本(供 CLI 与诊断使用)。
pub const LANGUAGE_NAME: &str = "aether";
pub const SPEC_VERSION: &str = "0.1.0-draft";

/// 源码位置:`offset` 为字节偏移,`line`/`col` 为 1 起算,`col` 按码点计数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pos {
    pub offset: usize,
    pub line: u32,
    pub col: u32,
}

impl Pos {
    /// 构造一个哑位置(用于无源码场景的测试与合成节点)。
    pub fn dummy() -> Self {
        Pos { offset: 0, line: 1, col: 1 }
    }
}

/// 源码区间,半开 `[start, end)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    /// 构造一个哑区间(用于无源码场景的测试与合成节点)。
    pub fn dummy() -> Self {
        Span { start: Pos::dummy(), end: Pos::dummy() }
    }

    /// 合并两个区间,取最早起点与最晚终点。
    pub fn merge(&self, other: &Span) -> Span {
        let start = if self.start.offset <= other.start.offset { self.start } else { other.start };
        let end = if self.end.offset >= other.end.offset { self.end } else { other.end };
        Span { start, end }
    }
}

/// AST 节点全局唯一编号;诊断与图补丁(远期)的锚点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

/// 一个源文件解析后的完整程序。
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub module: Module,
}

/// 模块:顶层项的有序集合。`name` 为 `(module 名 ...)` 形式的名字;
/// 无 module 形式时解析器合成匿名模块(`name = None`)。
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: Option<String>,
    pub items: Vec<Item>,
    pub span: Span,
}

/// 模块级顶层项。
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDef),
    Struct(StructDef),
    Let(LetDef),
    Expr(Expr),
}

/// 函数定义(具名或匿名;匿名函数 `name = None`,可作为表达式出现)。
#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    pub node_id: NodeId,
    pub name: Option<String>,
    pub params: Vec<Param>,
    pub ret_ty: Ty,
    /// 对应语法中的显式效果标记 `!`(首版仅解析保留,不做检查)。
    pub is_effectful: bool,
    pub contracts: Vec<Contract>,
    pub body: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
    pub span: Span,
}

/// 结构体定义;`invariants` 为 `:invariant` 约束表达式(可多个)。
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub node_id: NodeId,
    pub name: String,
    pub fields: Vec<Field>,
    pub invariants: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    pub span: Span,
}

/// 顶层/块级不可变绑定 `(let 名 类型 初值)`。
#[derive(Debug, Clone, PartialEq)]
pub struct LetDef {
    pub node_id: NodeId,
    pub name: String,
    pub ty: Ty,
    pub value: Box<Expr>,
    pub span: Span,
}

/// 契约:`(fn ... :pre e :post e)` 或 `(struct ... :invariant e)`。
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    pub kind: ContractKind,
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractKind {
    Pre,
    Post,
    Invariant,
}

/// 带位置的类型标注。
#[derive(Debug, Clone, PartialEq)]
pub struct Ty {
    pub kind: TyKind,
    pub span: Span,
}

/// 类型语法(01-syntax.md 的 TYPE 规则):
/// `()`=Unit、`Int/Float/Bool/Str/Ast`、`(Vec T)`、`(Map K V)`、
/// `(Option T)`、`(Result T E)`、`(Fn (T*) -> T)`、结构体名。
#[derive(Debug, Clone, PartialEq)]
pub enum TyKind {
    Unit,
    Int,
    Float,
    Bool,
    Str,
    Ast,
    VecOf(Box<Ty>),
    MapOf(Box<Ty>, Box<Ty>),
    Optional(Box<Ty>),
    ResultOf(Box<Ty>, Box<Ty>),
    Fn { params: Vec<Ty>, ret: Box<Ty> },
    Named(String),
}

/// 带节点编号与位置的表达式。
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub node_id: NodeId,
    pub kind: ExprKind,
    pub span: Span,
}

/// 表达式(01-syntax.md 的 expr 规则,逐条对应)。
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    /// 变量引用。
    Var(String),
    /// `(quote expr)` —— 同像性,返回 Ast 值。
    Quote(Box<Expr>),
    /// `(if cond then else)`,三臂必填。
    If { cond: Box<Expr>, then_branch: Box<Expr>, else_branch: Box<Expr> },
    /// `(block expr*)`,值 = 末表达式。
    Block(Vec<Expr>),
    /// `(vec expr*)`。
    VecLit(Vec<Expr>),
    /// `(map (k v)*)`。
    MapLit(Vec<(Expr, Expr)>),
    /// 匿名/具名函数表达式。
    Fn(FnDef),
    /// 调用 / 结构体构造 / 内建函数,头为标识符。
    Call { name: String, args: Vec<Expr> },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64, id: u64) -> Expr {
        Expr { node_id: NodeId(id), kind: ExprKind::Int(n), span: Span::dummy() }
    }

    fn var(name: &str, id: u64) -> Expr {
        Expr { node_id: NodeId(id), kind: ExprKind::Var(name.to_string()), span: Span::dummy() }
    }

    fn call(name: &str, args: Vec<Expr>, id: u64) -> Expr {
        Expr { node_id: NodeId(id), kind: ExprKind::Call { name: name.to_string(), args }, span: Span::dummy() }
    }

    #[test]
    fn identity() {
        assert_eq!(LANGUAGE_NAME, "aether");
        assert_eq!(SPEC_VERSION, "0.1.0-draft");
    }

    #[test]
    fn span_merge_takes_earliest_and_latest() {
        let a = Span {
            start: Pos { offset: 10, line: 1, col: 11 },
            end: Pos { offset: 20, line: 1, col: 21 },
        };
        let b = Span {
            start: Pos { offset: 5, line: 1, col: 6 },
            end: Pos { offset: 30, line: 2, col: 3 },
        };
        let m = a.merge(&b);
        assert_eq!(m.start.offset, 5);
        assert_eq!(m.end.offset, 30);
    }

    #[test]
    fn dummy_spans_are_stable() {
        assert_eq!(Span::dummy(), Span::dummy());
        assert_eq!(Pos::dummy().line, 1);
    }

    #[test]
    fn node_id_ordering() {
        assert!(NodeId(1) < NodeId(2));
        assert_eq!(NodeId(7).0, 7);
    }

    /// 构造与 examples/fib.ae 等价的 AST,锁定关键结构。
    #[test]
    fn fib_ast_shape() {
        // (fn fib (n Int) -> Int :pre (>= n 0) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
        let n_ty = Ty { kind: TyKind::Int, span: Span::dummy() };
        let pre = Contract {
            kind: ContractKind::Pre,
            expr: call(">=", vec![var("n", 2), int(0, 3)], 4),
            span: Span::dummy(),
        };
        let body = call(
            "if",
            vec![
                call("<", vec![var("n", 5), int(2, 6)], 7),
                var("n", 8),
                call(
                    "+",
                    vec![
                        call("fib", vec![call("-", vec![var("n", 9), int(1, 10)], 11)], 12),
                        call("fib", vec![call("-", vec![var("n", 13), int(2, 14)], 15)], 16),
                    ],
                    17,
                ),
            ],
            18,
        );
        let fib = FnDef {
            node_id: NodeId(1),
            name: Some("fib".to_string()),
            params: vec![Param { name: "n".to_string(), ty: n_ty.clone(), span: Span::dummy() }],
            ret_ty: n_ty,
            is_effectful: false,
            contracts: vec![pre],
            body: Box::new(body),
            span: Span::dummy(),
        };
        assert_eq!(fib.params.len(), 1);
        assert_eq!(fib.params[0].ty.kind, TyKind::Int);
        assert_eq!(fib.contracts[0].kind, ContractKind::Pre);
        match &*fib.body {
            Expr { kind: ExprKind::Call { name, .. }, .. } => assert_eq!(name, "if"),
            other => panic!("expected call, got {:?}", other),
        }
    }

    /// 构造与 examples/point.ae 等价的结构体,锁定字段与不变量。
    #[test]
    fn point_struct_shape() {
        let f_ty = Ty { kind: TyKind::Float, span: Span::dummy() };
        let pt = StructDef {
            node_id: NodeId(1),
            name: "Point".to_string(),
            fields: vec![
                Field { name: "x".to_string(), ty: f_ty.clone(), span: Span::dummy() },
                Field { name: "y".to_string(), ty: f_ty, span: Span::dummy() },
            ],
            invariants: vec![call("and", vec![
                call("finite?", vec![var("x", 2)], 3),
                call("finite?", vec![var("y", 4)], 5),
            ], 6)],
            span: Span::dummy(),
        };
        assert_eq!(pt.name, "Point");
        assert_eq!(pt.fields.len(), 2);
        assert_eq!(pt.invariants.len(), 1);
    }

    #[test]
    fn ty_kind_variants() {
        let int = Ty { kind: TyKind::Int, span: Span::dummy() };
        let vec_int = TyKind::VecOf(Box::new(int.clone()));
        let opt = TyKind::Optional(Box::new(Ty { kind: vec_int, span: Span::dummy() }));
        let fn_ty = TyKind::Fn { params: vec![int], ret: Box::new(Ty { kind: opt, span: Span::dummy() }) };
        assert_eq!(fn_ty, TyKind::Fn {
            params: vec![Ty { kind: TyKind::Int, span: Span::dummy() }],
            ret: Box::new(Ty {
                kind: TyKind::Optional(Box::new(Ty {
                    kind: TyKind::VecOf(Box::new(Ty { kind: TyKind::Int, span: Span::dummy() })),
                    span: Span::dummy(),
                })),
                span: Span::dummy(),
            }),
        });
    }

    #[test]
    fn debug_output_names_variants() {
        let e = int(42, 1);
        let dbg = format!("{:?}", e);
        assert!(dbg.contains("Int(42)"));
        assert!(dbg.contains("NodeId(1)"));
        let f = Expr { node_id: NodeId(2), kind: ExprKind::Float(1.5), span: Span::dummy() };
        assert!(format!("{:?}", f).contains("Float(1.5)"));
    }
}
