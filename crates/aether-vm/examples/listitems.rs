//! 调试:列出模块项。
use aether_ast::Item;
use aether_parser::parse;

fn main() {
    let src = std::fs::read_to_string("lib/aether-std.ae").expect("read");
    let p = parse(&src).expect("parse");
    println!("items: {}", p.module.items.len());
    for (i, it) in p.module.items.iter().enumerate() {
        match it {
            Item::Fn(f) => println!("  [{}] fn {:?}", i, f.name),
            Item::Struct(s) => println!("  [{}] struct {}", i, s.name),
            Item::Let(l) => println!("  [{}] let {}", i, l.name),
            Item::Expr(_) => println!("  [{}] expr", i),
        }
    }
}
