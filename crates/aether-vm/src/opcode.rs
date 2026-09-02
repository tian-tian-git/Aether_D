//! 字节码指令集与可执行单元(ADR-0002 方案 A)。
//!
//! 语义与树遍历解释器完全一致(02-semantics / 06-std);
//! 契约运行时检查编译为内联字节码(:pre 于函数入口,:post 于返回前,:invariant 于构造处)。

use std::rc::Rc;

use aether_ast::{FnDef, NodeId, Span};
use aether_interp::Value;

/// 指令(单字节操作码 + 定长操作数,小端)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    LoadConst(u16),
    LoadLocal(u16),
    /// 弹出栈顶并写入槽(用于 result 槽)。
    StoreLocal(u16),
    LoadUpvalue(u8),
    LoadGlobal(u16),
    DefineGlobal(u16),
    /// 创建闭包;上值按序从栈顶弹出。
    Closure(u16),
    /// 调用闭包:栈顶下方 argc 个实参 + 栈顶闭包。
    Call(u8),
    /// 直接调用内建(免去闭包包装):栈顶 argc 个实参。
    CallBuiltin(u16, u8),
    Return,
    Pop,
    Jump(i16),
    JumpIfFalse(i16),
    JumpIfTrue(i16),
    VecNew(u8),
    MapNew(u8),
    StructNew(u16, u8),
    RaisePre,
    RaisePost,
    RaiseInvariant,
    /// 高阶内建原生执行(谓词闭包在 VM 内联运行,不走解释器回退)。
    HofFilter,
    HofMap,
    HofFold,
}

impl Op {
    pub fn encode(self, out: &mut Vec<u8>) {
        let (code, a, b) = match self {
            Op::LoadConst(i) => (0x01, i, 0),
            Op::LoadLocal(i) => (0x02, i, 0),
            Op::StoreLocal(i) => (0x03, i, 0),
            Op::LoadUpvalue(i) => (0x04, i as u16, 0),
            Op::LoadGlobal(i) => (0x05, i, 0),
            Op::DefineGlobal(i) => (0x06, i, 0),
            Op::Closure(i) => (0x07, i, 0),
            Op::Call(i) => (0x08, i as u16, 0),
            Op::CallBuiltin(i, j) => (0x09, i, j as u16),
            Op::Return => (0x0A, 0, 0),
            Op::Pop => (0x0B, 0, 0),
            Op::Jump(i) => (0x0C, i as u16, 0),
            Op::JumpIfFalse(i) => (0x0D, i as u16, 0),
            Op::JumpIfTrue(i) => (0x14, i as u16, 0),
            Op::VecNew(i) => (0x0E, i as u16, 0),
            Op::MapNew(i) => (0x0F, i as u16, 0),
            Op::StructNew(i, j) => (0x10, i, j as u16),
            Op::RaisePre => (0x11, 0, 0),
            Op::RaisePost => (0x12, 0, 0),
            Op::RaiseInvariant => (0x13, 0, 0),
            Op::HofFilter => (0x15, 0, 0),
            Op::HofMap => (0x16, 0, 0),
            Op::HofFold => (0x17, 0, 0),
        };
        out.push(code);
        out.extend_from_slice(&a.to_le_bytes());
        out.extend_from_slice(&b.to_le_bytes());
    }

    pub fn decode(code: &[u8], ip: usize) -> (Op, usize) {
        let read = |i: usize| code[ip + i] as u16;
        let u16v = |i: usize| u16::from_le_bytes([code[ip + i], code[ip + i + 1]]);
        let i16v = |i: usize| i16::from_le_bytes([code[ip + i], code[ip + i + 1]]);
        let op = match code[ip] {
            0x01 => Op::LoadConst(u16v(1)),
            0x02 => Op::LoadLocal(u16v(1)),
            0x03 => Op::StoreLocal(u16v(1)),
            0x04 => Op::LoadUpvalue(read(1) as u8),
            0x05 => Op::LoadGlobal(u16v(1)),
            0x06 => Op::DefineGlobal(u16v(1)),
            0x07 => Op::Closure(u16v(1)),
            0x08 => Op::Call(read(1) as u8),
            0x09 => Op::CallBuiltin(u16v(1), read(3) as u8),
            0x0A => Op::Return,
            0x0B => Op::Pop,
            0x0C => Op::Jump(i16v(1)),
            0x0D => Op::JumpIfFalse(i16v(1)),
            0x14 => Op::JumpIfTrue(i16v(1)),
            0x0E => Op::VecNew(read(1) as u8),
            0x0F => Op::MapNew(read(1) as u8),
            0x10 => Op::StructNew(u16v(1), read(3) as u8),
            0x11 => Op::RaisePre,
            0x12 => Op::RaisePost,
            0x13 => Op::RaiseInvariant,
            0x15 => Op::HofFilter,
            0x16 => Op::HofMap,
            0x17 => Op::HofFold,
            _ => panic!("bad opcode {}", code[ip]),
        };
        (op, ip + 5)
    }
}

/// 调试信息:指令位置 → 源节点(诊断定位)。
#[derive(Debug, Clone, Copy)]
pub struct DebugEntry {
    pub ip: u32,
    pub node_id: NodeId,
    pub span: Span,
}

/// 字节码块。
#[derive(Debug, Clone)]
pub struct Chunk {
    pub code: Vec<u8>,
    pub consts: Vec<Value>,
    pub debug: Vec<DebugEntry>,
}

/// 上值来源:捕获父帧局部槽、传递祖父上值,或自引用(具名局部函数的递归)。
#[derive(Debug, Clone)]
pub enum UpvalueLoc {
    Local(u16),
    ParentUpvalue(u8),
    /// 具名局部函数的自引用:闭包创建时把自身写回环境(编译器压 Unit 占位)。
    SelfRef,
}

#[derive(Debug, Clone)]
pub struct UpvalueSrc {
    pub name: String,
    pub loc: UpvalueLoc,
}

/// 函数原型。
#[derive(Debug, Clone)]
pub struct Prototype {
    pub name: Option<String>,
    pub def: Rc<FnDef>,
    pub chunk: Chunk,
    pub nparams: u8,
    pub upvalues: Vec<UpvalueSrc>,
    pub nslots: u16,
}

/// 编译产物:整个模块。
#[derive(Debug)]
pub struct ModuleCompiled {
    /// 全局名表(模块级函数与 let 常量)。
    pub global_names: Vec<String>,
    pub struct_defs: Vec<Rc<aether_ast::StructDef>>,
    pub builtin_names: Vec<&'static str>,
    /// 全部原型(模块函数在前;嵌套函数按编译序)。
    pub protos: Vec<Rc<Prototype>>,
    /// 顶层初始化(let/表达式)。
    pub init: Prototype,
    /// main 在 protos 中的索引。
    pub main: Option<usize>,
}
