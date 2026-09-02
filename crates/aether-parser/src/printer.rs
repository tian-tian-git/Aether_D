//! 打印器:AST → 规范 S 表达式文本(06-std.md §0 值规范表示同源)。
//!
//! 约定:
//! - 输出为紧凑单行形式,唯一、可被解析器回读(round-trip);
//! - 空白不做保留:round-trip 语义由「parse → print → parse 不变」保证;
//! - `Float` 输出必须可回读为 Float(整数部分补 `.0`,指数形式补小数点)。

use aether_ast::{
    ContractKind, Expr, ExprKind, FnDef, Item, Program, StructDef, Ty, TyKind,
};

/// 打印整个程序。
pub fn print_program(p: &Program) -> String {
    let body = p.module.items.iter().map(print_item).collect::<Vec<_>>().join(" ");
    match &p.module.name {
        Some(name) if body.is_empty() => format!("(module {})", name),
        Some(name) => format!("(module {} {})", name, body),
        None => body,
    }
}

/// 打印模块级项。
pub fn print_item(item: &Item) -> String {
    match item {
        Item::Fn(f) => print_fn(f),
        Item::Struct(s) => print_struct(s),
        Item::Let(l) => format!("(let {} {} {})", l.name, print_ty(&l.ty), print_expr(&l.value)),
        Item::Expr(e) => print_expr(e),
    }
}

fn print_struct(s: &StructDef) -> String {
    let fields = s.fields.iter().map(|f| format!("({} {})", f.name, print_ty(&f.ty))).collect::<Vec<_>>().join(" ");
    let invariants = s
        .invariants
        .iter()
        .map(|e| format!(":invariant {}", print_expr(e)))
        .collect::<Vec<_>>()
        .join(" ");
    let mut parts = vec![format!("struct {}", s.name)];
    if !fields.is_empty() {
        parts.push(fields);
    }
    if !invariants.is_empty() {
        parts.push(invariants);
    }
    format!("({})", parts.join(" "))
}

/// 打印函数(具名/匿名,表达式或模块项共用)。
pub fn print_fn(f: &FnDef) -> String {
    let params = f.params.iter().map(|p| format!("({} {})", p.name, print_ty(&p.ty))).collect::<Vec<_>>().join(" ");
    let contracts = f
        .contracts
        .iter()
        .map(|c| {
            let kw = match c.kind {
                ContractKind::Pre => ":pre",
                ContractKind::Post => ":post",
                ContractKind::Invariant => ":invariant",
            };
            format!("{} {}", kw, print_expr(&c.expr))
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut parts = vec![match &f.name {
        Some(n) => format!("fn {}", n),
        None => "fn".to_string(),
    }];
    // 空参数列表规范表示为 ()
    parts.push(if params.is_empty() { "()".to_string() } else { params });
    parts.push("->".to_string());
    parts.push(print_ty(&f.ret_ty));
    if f.is_effectful {
        parts.push("!".to_string());
    }
    if !contracts.is_empty() {
        parts.push(contracts);
    }
    parts.push(print_expr(&f.body));
    format!("({})", parts.join(" "))
}

/// 打印类型。
pub fn print_ty(t: &Ty) -> String {
    match &t.kind {
        TyKind::Unit => "()".to_string(),
        TyKind::Int => "Int".to_string(),
        TyKind::Float => "Float".to_string(),
        TyKind::Bool => "Bool".to_string(),
        TyKind::Str => "Str".to_string(),
        TyKind::Ast => "Ast".to_string(),
        TyKind::VecOf(inner) => format!("(Vec {})", print_ty(inner)),
        TyKind::MapOf(k, v) => format!("(Map {} {})", print_ty(k), print_ty(v)),
        TyKind::Optional(inner) => format!("(Option {})", print_ty(inner)),
        TyKind::ResultOf(ok, err) => format!("(Result {} {})", print_ty(ok), print_ty(err)),
        TyKind::Fn { params, ret } => {
            let ps = params.iter().map(print_ty).collect::<Vec<_>>().join(" ");
            format!("(Fn ({}) -> {})", ps, print_ty(ret))
        }
        TyKind::Named(n) => n.clone(),
    }
}

/// 打印表达式。
pub fn print_expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(i) => i.to_string(),
        ExprKind::Float(f) => float_str(*f),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Str(s) => escape_str(s),
        ExprKind::Var(name) => name.clone(),
        ExprKind::Quote(inner) => format!("(quote {})", print_expr(inner)),
        ExprKind::If { cond, then_branch, else_branch } => {
            format!("(if {} {} {})", print_expr(cond), print_expr(then_branch), print_expr(else_branch))
        }
        ExprKind::Block(exprs) => {
            let body = exprs.iter().map(print_expr).collect::<Vec<_>>().join(" ");
            if body.is_empty() { "(block)".to_string() } else { format!("(block {})", body) }
        }
        ExprKind::VecLit(items) => {
            let body = items.iter().map(print_expr).collect::<Vec<_>>().join(" ");
            if body.is_empty() { "(vec)".to_string() } else { format!("(vec {})", body) }
        }
        ExprKind::MapLit(pairs) => {
            let body = pairs
                .iter()
                .map(|(k, v)| format!("({} {})", print_expr(k), print_expr(v)))
                .collect::<Vec<_>>()
                .join(" ");
            if body.is_empty() { "(map)".to_string() } else { format!("(map {})", body) }
        }
        ExprKind::Fn(f) => print_fn(f),
        ExprKind::Let(l) => format!("(let {} {} {})", l.name, print_ty(&l.ty), print_expr(&l.value)),
        ExprKind::Call { name, args } => {
            let body = args.iter().map(print_expr).collect::<Vec<_>>().join(" ");
            if body.is_empty() { format!("({})", name) } else { format!("({} {})", name, body) }
        }
    }
}

/// Float 规范表示:保证词法器回读为 Float(非 Int)。
/// 绝对值 ≥1e16 或 <1e-4 时用指数形式(与常用数值打印惯例一致),
/// 指数形式整数部分补 `.0`(词法器要求小数点两侧均有数字)。
fn float_str(f: f64) -> String {
    debug_assert!(f.is_finite(), "词法器应已拒绝非有限字面量");
    let abs = f.abs();
    let use_exp = abs != 0.0 && (abs >= 1e16 || abs < 1e-4);
    let s = if use_exp { format!("{:e}", f) } else { format!("{}", f) };
    if let Some((mantissa, exp)) = s.split_once('e') {
        if mantissa.contains('.') {
            format!("{}e{}", mantissa, exp)
        } else {
            format!("{}.0e{}", mantissa, exp)
        }
    } else if s.contains('.') {
        s
    } else {
        format!("{}.0", s)
    }
}

/// 字符串字面量转义(与词法器 01-syntax.md §2 互逆)。
fn escape_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:X}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    /// parse → print 的规范文本。
    fn canonical(src: &str) -> String {
        let p = parse(src).expect("parse failed");
        print_program(&p)
    }

    /// print → parse → print 不变(fixpoint)。
    fn assert_fixpoint(src: &str) {
        let first = canonical(src);
        let second = canonical(&first);
        assert_eq!(first, second, "printer is not a fixpoint for {:?}", src);
    }

    #[test]
    fn hello_canonical() {
        assert_eq!(
            canonical(r#"(module hello
  (fn main () -> () !
    (out "Hello, Aether!")))"#),
            r#"(module hello (fn main () -> () ! (out "Hello, Aether!")))"#
        );
    }

    #[test]
    fn fib_canonical() {
        let src = r#"(module fib
  (fn fib (n Int) -> Int
    :pre (>= n 0)
    (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))))"#;
        let out = canonical(src);
        assert!(out.starts_with("(module fib (fn fib (n Int) -> Int :pre (>= n 0) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))))"));
    }

    #[test]
    fn float_printing_roundtrips() {
        assert_eq!(float_str(3.0), "3.0");
        assert_eq!(float_str(-0.0), "-0.0");
        assert_eq!(float_str(3.14), "3.14");
        assert_eq!(float_str(0.5), "0.5");
        assert_eq!(float_str(1e300), "1.0e300");
        assert_eq!(float_str(1.5e-7), "1.5e-7");
    }

    #[test]
    fn string_escaping_roundtrips() {
        assert_eq!(escape_str("plain"), "\"plain\"");
        assert_eq!(escape_str("a\nb\t\"c\\d\u{1}"), "\"a\\nb\\t\\\"c\\\\d\\u{1}\"");
    }

    #[test]
    fn examples_roundtrip() {
        for path in [
            "../../examples/hello.ae",
            "../../examples/fib.ae",
            "../../examples/sort.ae",
            "../../examples/point.ae",
        ] {
            let src = std::fs::read_to_string(path).unwrap();
            assert_fixpoint(&src);
        }
    }

    #[test]
    fn all_forms_roundtrip() {
        let snippets = [
            "(quote (+ 1 2))",
            "(block)",
            "(vec 1 2 3)",
            "(map (\"a\" 1) (\"b\" 2))",
            "(fn f () -> () ! (out \"x\"))",
            "(fn (x Int) -> Bool (>= x 0))",
            "(struct S (x Int) :invariant (>= x 0))",
            "(let x (Vec (Option Int)) (vec (some 1) none))",
            "(if true (block (let y Float 1.5) y) 0.0)",
            "(fn g (f (Fn (Int) -> Bool)) -> (Map Str (Vec Float)) (block))",
        ];
        for s in snippets {
            assert_fixpoint(s);
        }
    }

    #[test]
    fn struct_and_map_printing() {
        assert_eq!(canonical("(struct S (x Int) :invariant (>= x 0))"), "(struct S (x Int) :invariant (>= x 0))");
        assert_eq!(canonical(r#"(map ("a" 1) ("b" 2))"#), r#"(map ("a" 1) ("b" 2))"#);
    }
}
