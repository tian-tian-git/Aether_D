//! M6 自举测试:Aether 标准库(用 Aether 自身实现)在 VM 上执行。

use aether_parser::parse;
use aether_vm::Vm;

fn std_src() -> String {
    std::fs::read_to_string("../../lib/aether-std.ae").expect("read std")
}

/// 去掉演示 main,追加测试 main(并闭合 module),运行并返回退出码。
fn run_with(extra_main: &str) -> Result<i32, String> {
    let base = std_src();
    // 用测试 main 替换演示 main:取最后一个 (fn main ...) 之前的文本
    let cut = base.rfind("(fn main").expect("find main");
    // base[..cut] 处 module 尚未闭合 → 追加测试 main 后补一个 ')' 闭合 module
    let src = format!("{}\n{}\n)\n", &base[..cut], extra_main);
    let program = parse(&src).map_err(|d| format!("parse: {}", d.message))?;
    let mut vm = Vm::from_program(&program).map_err(|d| format!("vm: {}", d.message))?;
    vm.run_main(vec![]).map_err(|d| format!("run: {}", d.message))
}

fn ok(main: &str) -> i32 {
    run_with(main).unwrap_or_else(|e| panic!("expected success, got: {}", e))
}

#[test]
fn std_passes_static_check() {
    let src = std_src();
    let program = parse(&src).expect("parse");
    aether_verify::check_program(&program).expect("typecheck");
    // 静态契约验证:标准库应无证实的违反(unsupported 域静默跳过)
    if let Ok(api) = aether_verify::try_load() {
        let diags = aether_verify::verify_program(&api, &program);
        assert!(diags.is_empty(), "std has provable violations: {:?}", diags);
    }
}

#[test]
fn aggregate_functions() {
    let main = r#"(fn main () -> Int
  (block
    (let xs (Vec Int) (vec 1 2 3 4))
    (if (== (sum xs) 10)
        (if (== (product xs) 24)
            (if (== (average xs) 2)
                (if (== (count (vec 1 2 1) 1) 2)
                    (if (contains (vec 1 2 3) 2)
                        (if (all? xs (fn (x Int) -> Bool (> x 0)))
                            (if (any? xs (fn (x Int) -> Bool (== x 3))) 0 1) 1) 1) 1) 1) 1) 1)))"#;
    assert_eq!(ok(main), 0);
}

#[test]
fn sequence_functions() {
    let main = r#"(fn main () -> Int
  (block
    (let xs (Vec Int) (vec 1 2 3 4 5))
    (let rev (Vec Int) (reverse xs))
    (let tk (Vec Int) (take xs 3))
    (let dp (Vec Int) (drop xs 2))
    (if (== (head rev) 5)
        (if (== (len tk) 3)
            (if (== (head dp) 3) 0 1) 1) 1)))"#;
    assert_eq!(ok(main), 0);
}

#[test]
fn sort_and_search_functions() {
    let main = r#"(fn main () -> Int
  (block
    (let xs (Vec Int) (vec 5 3 1 4 2))
    (let sorted (Vec Int) (qsort xs))
    (if (== (head sorted) 1)
        (if (== (kth xs 2) 3)
            (if (== (bsearch sorted 4) 3)
                (if (== (bsearch sorted 9) -1) 0 1) 1) 1) 1)))"#;
    assert_eq!(ok(main), 0);
}

#[test]
fn insert_into_sorted() {
    let main = r#"(fn main () -> Int
  (block
    (let s1 (Vec Int) (insert (vec 1 3 5) 4))
    (let s2 (Vec Int) (insert s1 0))
    (if (and (== (len s2) 5) (== (get s2 0) 0)) 0 1)))"#;
    assert_eq!(ok(main), 0);
}

#[test]
fn math_functions() {
    let main = r#"(fn main () -> Int
  (if (== (dot (vec 1 2 3) (vec 4 5 6)) 32)
      (if (== (power 2 10) 1024)
          (if (== (fact 5) 120)
              (if (== (fib 10) 55) 0 1) 1) 1) 1))"#;
    assert_eq!(ok(main), 0);
}

#[test]
fn take_pre_violation_caught_statically() {
    // take 的 :pre (>= n 0) 在 n=-1 时被静态证实违反
    let base = std_src();
    let cut = base.rfind("(fn main").expect("find main");
    let src = format!("{}\n(fn main () -> Int (block (let t (Vec Int) (take (vec 1) -1)) 0))\n)\n", &base[..cut]);
    let program = parse(&src).expect("parse");
    let api = match aether_verify::try_load() {
        Ok(a) => a,
        Err(_) => return, // Z3 不可用则跳过
    };
    let diags = aether_verify::verify_program(&api, &program);
    assert!(diags.iter().any(|d| d.code == "E4001"), "expected E4001, got {:?}", diags);
}
