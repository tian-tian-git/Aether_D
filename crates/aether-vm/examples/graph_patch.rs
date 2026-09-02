//! M6 WP6.2:图补丁 / 热更新 demo。
//!
//! 演示「代码图补丁」闭环:
//! 1. 解析程序为 AST(带 NodeId 的节点图);
//! 2. **节点级替换**(按 NodeId 定位,直接改 AST 节点,不做文本编辑);
//! 3. 重打印规范源码 → 重解析 → 重执行(同一进程内热更新)。

use aether_ast::{Expr, ExprKind, Item};
use aether_parser::{parse, print_program};
use aether_vm::Vm;

fn main() {
    let src = "(module t
  (fn f (x Int) -> Int (+ x 1))
  (fn main () -> Int (f 41)))";

    let mut program = parse(src).expect("parse");
    let before = print_program(&program);

    // AI 式修补:定位「f 函数体中的 (+ x 1)」,把常量 1 换成 2
    let mut anchor: Option<u64> = None;
    for item in &mut program.module.items {
        if let Item::Fn(f) = item {
            patch_expr(&mut f.body, &mut anchor);
        }
    }
    let after = print_program(&program);

    println!("== 原始程序 ==");
    println!("{}", before);
    println!("== 修补后(NodeId {} 处常量 1 → 2)==\n", anchor.expect("patched node"));
    println!("{}", after);

    // 热更新:同一进程内,先跑旧版,再跑新版(重新编译执行,无需重启进程)
    let mut vm_before = Vm::from_program(&parse(&before).expect("parse")).expect("vm");
    let r1 = vm_before.run_main(vec![]).expect("run");
    let mut vm_after = Vm::from_program(&parse(&after).expect("parse")).expect("vm");
    let r2 = vm_after.run_main(vec![]).expect("run");
    println!("\nrun(before) = {}", r1);
    println!("run(after)  = {}", r2);
    assert_eq!(r1, 42);
    assert_eq!(r2, 43);
    println!("\n图补丁闭环验证通过:节点级修改 → 规范重打印 → 热更新执行");
}

fn patch_expr(e: &mut Expr, anchor: &mut Option<u64>) {
    if anchor.is_some() {
        return;
    }
    match &mut e.kind {
        ExprKind::Call { name, args } => {
            if name == "+" && args.len() == 2 {
                if let ExprKind::Int(1) = args[1].kind {
                    args[1].kind = ExprKind::Int(2);
                    *anchor = Some(args[1].node_id.0);
                    return;
                }
            }
            for a in args {
                patch_expr(a, anchor);
            }
        }
        ExprKind::Block(es) => {
            for x in es {
                patch_expr(x, anchor);
            }
        }
        ExprKind::If { cond, then_branch, else_branch } => {
            patch_expr(cond, anchor);
            patch_expr(then_branch, anchor);
            patch_expr(else_branch, anchor);
        }
        ExprKind::Let(l) => patch_expr(&mut l.value, anchor),
        ExprKind::Quote(inner) => patch_expr(inner, anchor),
        _ => {}
    }
}
