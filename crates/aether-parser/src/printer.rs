//! Aether 前端:词法 → 解析 → 打印。
//!
//! 打印器实现位于 `aether-ast::printer`(供解释器复用);
//! 本模块保持 `print_program` 等导出与 round-trip 测试。

pub use aether_ast::printer::*;

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
            "(map-of (\"a\" 1) (\"b\" 2))",
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
        assert_eq!(canonical(r#"(map-of ("a" 1) ("b" 2))"#), r#"(map-of ("a" 1) ("b" 2))"#);
    }
}
