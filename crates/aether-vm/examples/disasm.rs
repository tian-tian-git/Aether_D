//! 调试反汇编:打印 main 原型字节码。
use aether_parser::parse;
use aether_vm::compiler::compile;
use aether_vm::opcode::Op;

fn main() {
    let src = std::env::args().nth(1).expect("need source file");
    let program = parse(&src).expect("parse failed");
    let mc = compile(&program).expect("compile failed");
    for (i, p) in mc.protos.iter().enumerate() {
        println!("=== proto {} {:?} (nparams={}, nslots={}, upvalues={:?})", i, p.name, p.nparams, p.nslots, p.upvalues);
        let mut ip = 0;
        while ip < p.chunk.code.len() {
            let (op, next) = Op::decode(&p.chunk.code, ip);
            println!("  {:04} {:?}", ip, op);
            ip = next;
        }
    }
    println!("=== init ===");
    let mut ip = 0;
    while ip < mc.init.chunk.code.len() {
        let (op, next) = Op::decode(&mc.init.chunk.code, ip);
        println!("  {:04} {:?}", ip, op);
        ip = next;
    }
}
