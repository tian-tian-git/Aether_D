//! VM 集成测试:与树遍历解释器同一套语义用例(02-semantics / 06-std)。

use aether_parser::parse;
use aether_vm::Vm;

fn run(src: &str, args: Vec<String>) -> Result<i32, String> {
    let program = parse(src).map_err(|d| format!("parse: {}", d.message))?;
    let mut vm = Vm::from_program(&program).map_err(|d| format!("vm: {}", d.message))?;
    vm.run_main(args).map_err(|d| format!("run: {}", d.message))
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
fn map_filter_fold_pipeline() {
    let src = r#"(module t
  (fn main () -> Int
    (fold
      (fn (acc Int) (x Int) -> Int (+ acc x))
      0
      (map
        (fn (x Int) -> Int (* x x))
        (filter (fn (x Int) -> Bool (== (% x 2) 0)) (range 1 11))))))"#;
    assert_eq!(run_ok(src), 220);
}

#[test]
fn struct_field_access_and_invariant() {
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
fn division_by_zero_is_e5002() {
    assert!(run_err("(fn main () -> Int (/ 1 0))").contains("division by zero"));
}

#[test]
fn empty_head_is_e5003() {
    assert!(run_err("(fn main () -> Int (head (vec)))").contains("'head' on an empty Vec"));
}

#[test]
fn unknown_function_is_e5008() {
    assert!(run_err("(fn main () -> Int (nope 1))").contains("unknown global 'nope'"));
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
fn recursive_local_fn() {
    // 局部具名函数递归(占槽先行的自引用)
    let src = r#"(module t
  (fn main () -> Int
    (block
      (let g (Fn (Int) -> Int) (fn g (n Int) -> Int (if (<= n 1) 1 (* n (g (- n 1))))))
      (g 5))))"#;
    assert_eq!(run_ok(src), 120);
}
