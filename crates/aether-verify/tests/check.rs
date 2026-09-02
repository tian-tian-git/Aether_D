//! 类型检查集成测试(对照 03-types.md)。

use aether_parser::parse;
use aether_verify::check_program;

fn check_ok(src: &str) {
    let p = parse(src).unwrap_or_else(|e| panic!("parse failed: {}", e.message));
    if let Err(d) = check_program(&p) {
        panic!("expected typecheck success, got [{}] {}", d.code, d.message);
    }
}

fn check_err(src: &str) -> String {
    let p = parse(src).unwrap_or_else(|e| panic!("parse failed: {}", e.message));
    match check_program(&p) {
        Err(d) => format!("{}: {}", d.code, d.message),
        Ok(()) => panic!("expected typecheck error for {:?}", src),
    }
}

#[test]
fn examples_pass() {
    for path in [
        "../../examples/hello.ae",
        "../../examples/fib.ae",
        "../../examples/sort.ae",
        "../../examples/point.ae",
    ] {
        let src = std::fs::read_to_string(path).unwrap();
        check_ok(&src);
    }
}

#[test]
fn basic_functions_pass() {
    check_ok("(fn add (a Int) (b Int) -> Int (+ a b))");
    check_ok("(fn id (x Str) -> Str x)");
    check_ok("(fn main () -> Int 0)");
    check_ok("(fn main (args (Vec Str)) -> Int (len args))");
}

#[test]
fn generics_infer_through_builtins() {
    check_ok("(fn main () -> Int (+ (head (vec 1 2)) 1))");
    check_ok("(fn f () -> Str (head (vec \"a\")))");
    check_ok("(fn main () -> Int (fold (fn (a Int) (x Int) -> Int (+ a x)) 0 (range 0 10)))");
    check_ok("(fn f () -> (Vec Int) (map (fn (x Int) -> Int (* x 2)) (range 0 3)))");
    check_ok("(fn f () -> (Vec Int) (filter (fn (x Int) -> Bool (> x 0)) (range -2 2)))");
}

#[test]
fn struct_field_access_types() {
    let src = "(struct P (x Float) (y Int)) (fn f (p P) -> Float (. p \"x\"))";
    check_ok(src);
}

#[test]
fn named_fn_as_value() {
    let src = "(fn is-even (x Int) -> Bool (== (% x 2) 0)) (fn main () -> Int (len (filter is-even (range 0 10))))";
    check_ok(src);
}

#[test]
fn contract_must_be_bool() {
    assert!(check_err("(fn f (n Int) -> Int :pre 1 n)").starts_with("E3007"));
    assert!(check_err("(fn f (n Int) -> Int :post 1 n)").starts_with("E3007"));
    assert!(check_err("(struct S (x Int) :invariant 1)").starts_with("E3007"));
}

#[test]
fn post_contract_result_typed() {
    check_ok("(fn q (xs (Vec Int)) -> (Vec Int) :post (sorted? result) xs)");
}

#[test]
fn if_condition_must_be_bool() {
    assert!(check_err("(fn main () -> Int (if 1 2 3))").starts_with("E3007"));
}

#[test]
fn type_mismatch_reported() {
    assert!(check_err("(fn f (x Int) -> Int \"s\")").starts_with("E3001"));
    assert!(check_err("(fn main () -> Int (+ 1 2.0))").starts_with("E3001"));
    assert!(check_err("(fn main () -> Int (block (let x Str 1) 0))").starts_with("E3001"));
    assert!(check_err("(fn main () -> Int (block (let x Int 1) (let x Int 2) x))").starts_with("E3002"));
}

#[test]
fn undefined_names_reported() {
    assert!(check_err("(fn f () -> Int y)").starts_with("E3003"));
    assert!(check_err("(fn main () -> Int (g 1))").starts_with("E3003"));
}

#[test]
fn unknown_type_reported() {
    assert!(check_err("(fn f (x Foo) -> Int 1)").starts_with("E3004"));
}

#[test]
fn arity_reported() {
    assert!(check_err("(fn main () -> Int (head (vec 1) 2))").starts_with("E3005"));
    assert!(check_err("(fn main () -> Int (abs 1 2))").starts_with("E3005"));
}

#[test]
fn unknown_field_reported() {
    let src = "(struct P (x Int)) (fn f (p P) -> Int (. p \"z\"))";
    assert!(check_err(src).starts_with("E3006"));
    let src2 = "(struct P (x Int)) (fn f (p P) (fname Str) -> Int (. p fname))";
    assert!(check_err(src2).starts_with("E3006"));
}

#[test]
fn main_return_must_be_unit_or_int() {
    assert!(check_err("(fn main () -> Str \"x\")").starts_with("E3008"));
    assert!(check_err("(fn main (a Int) -> Int 1)").starts_with("E3008"));
}

#[test]
fn duplicate_top_level_names_reported() {
    assert!(check_err("(fn f () -> Int 1) (fn f () -> Int 2)").starts_with("E3002"));
    assert!(check_err("(struct S (x Int)) (struct S (y Int))").starts_with("E3002"));
}

#[test]
fn block_let_scope_and_shadowing() {
    // 嵌套作用域遮蔽允许;同作用域重复不允许
    check_ok("(fn main () -> Int (block (let x Int 1) (block (let x Int 2) x)))");
    assert!(check_err("(fn main () -> Int (block (let x Int 1) (let x Int 2) x))").starts_with("E3002"));
}

#[test]
fn option_and_result_types() {
    check_ok("(fn main () -> Int (unwrap (some 7)))");
    check_ok("(fn main () -> Int (if (is-some? none) 1 0))");
    check_ok("(fn f () -> Str (unwrap (ok \"fine\")))");
    assert!(check_err("(fn main () -> Int (unwrap (some \"s\")))").starts_with("E3001"));
}

#[test]
fn quote_is_ast() {
    check_ok("(fn f () -> Str (ast->str (quote (+ 1 2))))");
    assert!(check_err("(fn main () -> Int (ast->str 1))").starts_with("E3001"));
}
