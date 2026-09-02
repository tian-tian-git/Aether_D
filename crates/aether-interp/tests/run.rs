//! 解释器集成测试:golden 执行 + 运行时错误路径(对照 02-semantics / 06-std)。

use aether_interp::Interp;
use aether_parser::parse;

fn run(src: &str, args: Vec<String>) -> Result<i32, String> {
    let program = parse(src).map_err(|d| format!("parse: {}", d.message))?;
    Interp::new().run_program(&program, args).map_err(|d| format!("run: {}", d.message))
}

fn run_ok(src: &str) -> i32 {
    run(src, vec![]).unwrap_or_else(|e| panic!("expected success, got: {}", e))
}

fn run_err(src: &str) -> String {
    match run(src, vec![]) {
        Err(e) => e,
        Ok(code) => panic!("expected error, got exit code {}", code),
    }
}

#[test]
fn hello_runs() {
    let code = run_ok(r#"(module hello (fn main () -> () ! (out "Hello, Aether!")))"#);
    assert_eq!(code, 0);
}

#[test]
fn fib_recursion() {
    let src = r#"(module t
  (fn fib (n Int) -> Int
    :pre (>= n 0)
    (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
  (fn main () -> Int (fib 10)))"#;
    assert_eq!(run_ok(src), 55);
}

#[test]
fn qsort_with_post_contract() {
    let src = r#"(module t
  (fn qsort (xs (Vec Int)) -> (Vec Int)
    :post (sorted? result)
    (if (empty? xs) xs
        (concat (qsort (filter (fn (x Int) -> Bool (< x (head xs))) xs))
                (vec (head xs))
                (qsort (filter (fn (x Int) -> Bool (> x (head xs))) xs)))))
  (fn main () -> Int (head (qsort (vec 5 3 1 4 2)))))"#;
    assert_eq!(run_ok(src), 1);
}

#[test]
fn closures_capture_lexical_env() {
    let src = r#"(module t
  (fn make-adder (n Int) -> (Fn (Int) -> Int)
    (fn (x Int) -> Int (+ x n)))
  (fn main () -> Int
    (block
      (let add10 (Fn (Int) -> Int) (make-adder 10))
      (add10 5))))"#;
    assert_eq!(run_ok(src), 15);
}

#[test]
fn map_filter_fold_pipeline() {
    let src = r#"(module t
  (fn main () -> Int
    (fold
      (fn (acc Int) (x Int) -> Int (+ acc x))
      0
      (map
        (fn (x Int) -> Int (* x x))
        (filter (fn (x Int) -> Bool (== (% x 2) 0)) (range 1 11))))))"#;
    // 2^2 + 4^2 + 6^2 + 8^2 + 10^2 = 4+16+36+64+100 = 220
    assert_eq!(run_ok(src), 220);
}

#[test]
fn struct_field_access_and_effects() {
    let src = r#"(module t
  (struct Point (x Float) (y Float)
    :invariant (and (finite? x) (finite? y)))
  (fn main () -> Int
    (block
      (let p Point (Point 3.0 4.0))
      (let d Float (sqrt (+ (* (. p "x") (. p "x")) (* (. p "y") (. p "y")))))
      (if (== d 5.0) 0 1))))"#;
    assert_eq!(run_ok(src), 0);
}

#[test]
fn quote_and_ast_to_str() {
    let src = r#"(module t
  (fn main () -> Int
    (if (== (ast->str (quote (+ 1 2))) "(+ 1 2)") 0 1)))"#;
    assert_eq!(run_ok(src), 0);
}

#[test]
fn main_with_args() {
    let src = r#"(module t
  (fn main (args (Vec Str)) -> Int (len args)))"#;
    assert_eq!(run(src, vec!["a".into(), "b".into(), "c".into()]).unwrap(), 3);
}

#[test]
fn options_and_results() {
    let src = r#"(module t
  (fn main () -> Int
    (if (is-none? none) (if (== (unwrap (some 7)) 7) (if (is-err? (err "x")) 0 9) 9) 9)))"#;
    assert_eq!(run_ok(src), 0);
}

#[test]
fn map_literals_and_keys() {
    let src = r#"(module t
  (fn main () -> Int
    (block
      (let m (Map Str Int) (map-of ("a" 1) ("b" 2)))
      (if (has? m "b") (+ (get m "a") (get m "b")) 0))))"#;
    assert_eq!(run_ok(src), 3);
}

#[test]
fn pre_contract_violation_is_e4001() {
    let src = r#"(module t
  (fn fib (n Int) -> Int :pre (>= n 0) n)
  (fn main () -> Int (fib -1)))"#;
    assert!(run_err(src).contains(":pre contract violated"), "got: {}", run_err(src));
}

#[test]
fn post_contract_violation_is_e4002() {
    let src = r#"(module t
  (fn bad () -> Int :post false 1)
  (fn main () -> Int (bad)))"#;
    assert!(run_err(src).contains(":post contract violated"));
}

#[test]
fn invariant_violation_is_e4003() {
    let src = r#"(module t
  (struct S (x Int) :invariant (>= x 0))
  (fn main () -> Int (block (let s S (S -1)) 0)))"#;
    assert!(run_err(src).contains(":invariant violation"));
}

#[test]
fn unbound_variable_is_e5001() {
    assert!(run_err("(fn main () -> Int mystery)").contains("unbound variable 'mystery'"));
}

#[test]
fn division_by_zero_is_e5002() {
    assert!(run_err("(fn main () -> Int (/ 1 0))").contains("division by zero"));
    assert!(run_err("(fn main () -> Int (/ 1.0 0.0))").contains("division by zero"));
}

#[test]
fn empty_head_is_e5003() {
    assert!(run_err("(fn main () -> Int (head (vec)))").contains("'head' on an empty Vec"));
}

#[test]
fn out_of_bounds_is_e5004() {
    assert!(run_err("(fn main () -> Int (get (vec 1) 5))").contains("out of bounds"));
    assert!(run_err("(fn main () -> Int (get (map-of (\"a\" 1)) \"b\"))").contains("not found in Map"));
}

#[test]
fn type_mismatch_is_e5005() {
    assert!(run_err("(fn main () -> Int (+ 1 2.0))").contains("cannot mix Int and Float"));
    assert!(run_err("(fn main () -> Int (if 1 2 3))").contains("if condition must be Bool"));
}

#[test]
fn unknown_function_is_e5008() {
    assert!(run_err("(fn main () -> Int (nope 1))").contains("unknown function 'nope'"));
}

#[test]
fn duplicate_binding_is_e5013() {
    assert!(run_err("(fn main () -> Int (block (let x Int 1) (let x Int 2) x))").contains("duplicate binding 'x'"));
}

#[test]
fn unwrap_on_none_is_e5007() {
    assert!(run_err("(fn main () -> Int (unwrap none))").contains("unwrap on none"));
}

#[test]
fn str_to_int_rejects_garbage_is_e5010() {
    assert!(run_err("(fn main () -> Int (str->int \"4.2\"))").contains("not a decimal integer literal"));
}

#[test]
fn range_limit_is_e5014() {
    assert!(run_err("(fn main () -> Int (len (range 0 2000000)))").contains("exceeds the limit"));
}

#[test]
fn sqrt_domain_is_e5009() {
    assert!(run_err("(fn main () -> Int (block (let f Float (sqrt -1.0)) 0))").contains("domain error"));
}

#[test]
fn no_main_is_e5015() {
    assert!(run_err("(fn f () -> Int 1)").contains("no 'main' function"));
}

#[test]
fn int_arithmetic_wraps() {
    let src = "(fn main () -> Int (+ 9223372036854775807 1))";
    assert_eq!(run_ok(src), i64::MIN as i32);
}

#[test]
fn map_keys_put_roundtrip() {
    let src = r#"(module t
  (fn main () -> Int
    (block
      (let m1 (Map Str Int) (map-of ("a" 1)))
      (let m2 (Map Str Int) (put m1 "b" 2))
      (let ks (Vec Str) (keys m2))
      (if (== (len ks) 2) (+ (get m2 "a") (get m2 "b")) 0))))"#;
    assert_eq!(run_ok(src), 3);
}

#[test]
fn string_concat_and_conversions() {
    let src = r#"(module t
  (fn main () -> Int
    (block
      (let s Str (concat "a" "b" "c"))
      (let n Int (str->int "42"))
      (let back Str (int->str 42))
      (if (and (== s "abc") (== n 42) (== back "42") (== (str-len s) 3)) 0 1))))"#;
    assert_eq!(run_ok(src), 0);
}

#[test]
fn permutation_predicate() {
    let src = r#"(module t
  (fn main () -> Int
    (if (and (permutation? (vec 1 2 3) (vec 3 2 1))
             (not (permutation? (vec 1 2) (vec 1 3))))
        0 1)))"#;
    assert_eq!(run_ok(src), 0);
}

#[test]
fn empty_range_and_negative_range() {
    let src = r#"(module t
  (fn main () -> Int
    (if (and (== (len (range 5 5)) 0) (== (len (range 5 2)) 0)) 0 1)))"#;
    assert_eq!(run_ok(src), 0);
}

#[test]
fn nested_closures_chain() {
    let src = r#"(module t
  (fn make-mult (m Int) -> (Fn (Int) -> Int)
    (fn (x Int) -> Int (* x m)))
  (fn compose (f (Fn (Int) -> Int)) (g (Fn (Int) -> Int)) -> (Fn (Int) -> Int)
    (fn (x Int) -> Int (f (g x))))
  (fn main () -> Int
    (block
      (let double (Fn (Int) -> Int) (make-mult 2))
      (let triple (Fn (Int) -> Int) (make-mult 3))
      (let six-x (Fn (Int) -> Int) (compose double triple))
      (six-x 10))))"#;
    assert_eq!(run_ok(src), 60);
}

#[test]
fn effects_are_explicitly_marked_but_not_enforced() {
    // v0.1:! 标记仅解析保留;调用 out 不强制调用方标记
    let src = r#"(module t
  (fn say () -> () (out "hi"))
  (fn main () -> Int (block (say) 0)))"#;
    assert_eq!(run_ok(src), 0);
}
