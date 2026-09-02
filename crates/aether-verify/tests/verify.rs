//! 静态契约验证集成测试(对照 04-contracts.md)。
//! 需要 Z3(本机 pip z3-solver);Z3 不可用则整体跳过并打印说明。

use aether_parser::parse;
use aether_verify::{check_program, try_load, verify_program};

fn verify(src: &str) -> Vec<String> {
    let p = parse(src).expect("parse failed");
    check_program(&p).expect("typecheck failed");
    let api = match try_load() {
        Ok(api) => api,
        Err(reason) => {
            eprintln!("SKIPPED (Z3 unavailable: {})", reason);
            return vec![];
        }
    };
    verify_program(&api, &p).iter().map(|d| format!("{}: {}", d.code, d.message)).collect()
}

#[test]
fn pre_violation_at_call_site() {
    let src = r#"(module t
  (fn nth (xs (Vec Int)) (i Int) -> Int
    :pre (and (>= i 0) (< i (len xs)))
    (get xs i))
  (fn main () -> Int
    (block
      (let xs (Vec Int) (vec 1 2 3))
      (nth xs 5))))"#;
    let ds = verify(src);
    assert!(ds.iter().any(|d| d.starts_with("E4001")), "got: {:?}", ds);
}

#[test]
fn post_violation_is_proved() {
    let src = "(fn bad () -> Int :post false 1)";
    let ds = verify(src);
    assert!(ds.iter().any(|d| d.starts_with("E4002")), "got: {:?}", ds);
}

#[test]
fn out_of_bounds_is_proved() {
    let src = "(fn f (xs (Vec Int)) -> Int :pre (== (len xs) 0) (get xs 0))";
    let ds = verify(src);
    assert!(ds.iter().any(|d| d.starts_with("E5004")), "got: {:?}", ds);
}

#[test]
fn head_on_empty_is_proved() {
    let src = "(fn f (xs (Vec Int)) -> Int :pre (== (len xs) 0) (head xs))";
    let ds = verify(src);
    assert!(ds.iter().any(|d| d.starts_with("E5004")), "got: {:?}", ds);
}

#[test]
fn safe_program_has_no_diagnostics() {
    let src = r#"(module t
  (fn nth (xs (Vec Int)) (i Int) -> Int
    :pre (and (>= i 0) (< i (len xs)))
    (get xs i))
  (fn inc (x Int) -> Int :post (> result x) (+ x 1))
  (fn main () -> Int
    (block
      (let xs (Vec Int) (vec 1 2 3))
      (let a Int (nth xs 1))
      (let b Int (inc a))
      b)))"#;
    let ds = verify(src);
    assert!(ds.is_empty(), "expected no diagnostics, got: {:?}", ds);
}

#[test]
fn if_branches_are_explored() {
    // 一条分支可达越界:should prove
    let src = r#"(fn f (xs (Vec Int)) (pick Bool) -> Int
    :pre (== (len xs) 1)
    (if pick (get xs 0) (get xs 1)))"#;
    let ds = verify(src);
    assert!(ds.iter().any(|d| d.starts_with("E5004")), "got: {:?}", ds);
}

#[test]
fn unsupported_domains_fall_back_silently() {
    // Map/Str/结构体等不支持静态:不误报
    let src = r#"(fn f (m (Map Str Int)) -> Int :pre (has? m "a") (get m "a"))"#;
    let ds = verify(src);
    assert!(ds.is_empty(), "unsupported domains must not report, got: {:?}", ds);
}
