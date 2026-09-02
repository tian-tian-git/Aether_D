//! 字节码执行器(栈式 VM,迭代帧,无宿主栈递归)。
//!
//! 与树遍历解释器共享:值模型(Value)、内建实现(Host)、错误码。
//! 设计说明:
//! - 局部变量 = 栈槽;闭包上值在创建时按名拷入词法 Env(语言无赋值,无需可变单元);
//! - 高阶内建(filter/map/fold)的回调经树遍历解释器执行(v0.1 简化:谓词为纯函数,
//!   语义与主路径一致;M5.5 可改为字节码内联,见 ADR-0002)。

use std::collections::HashMap;
use std::rc::Rc;

use aether_ast::{Expr, ExprKind, Item, NodeId, Span, StructDef};
use aether_diagnostic::Diagnostic;
use aether_interp::builtins::{builtins, BuiltinFn, Host};
use aether_interp::value::{repr, Env, FnValue, Value};
use aether_interp::Interp;

use crate::compiler::compile;
use crate::opcode::{DebugEntry, Op, Prototype};

pub const E4001: &str = "E4001";
pub const E4002: &str = "E4002";
pub const E4003: &str = "E4003";
pub const E5001: &str = "E5001";
pub const E5005: &str = "E5005";
pub const E5008: &str = "E5008";
pub const E5012: &str = "E5012";
pub const E5015: &str = "E5015";

pub struct Vm {
    globals: HashMap<String, Value>,
    global_names: Vec<String>,
    structs: Vec<Rc<StructDef>>,
    protos: Vec<Rc<Prototype>>,
    /// 与编译期内建名表对齐的函数指针表(免 HashMap 查表)。
    builtin_table: Vec<BuiltinFn>,
    frames: Vec<Frame>,
    stack: Vec<Value>,
    /// 当前内建调用点(诊断定位;Host 回调时合成 call-site)。
    builtin_site: Option<(NodeId, Span)>,
}

struct Frame {
    proto: Rc<Prototype>,
    base: usize,
    ip: usize,
    env: Rc<Env>,
}

impl Vm {
    /// 从 AST 编译并构造 VM(解析由调用方完成)。
    pub fn from_program(program: &aether_ast::Program) -> Result<Vm, Diagnostic> {
        let mc = compile(program).map_err(|e| {
            Diagnostic::error("E6002", format!("compile error: {}", e), program.module.span)
                .with_hint("this is an internal compiler bug — report it")
        })?;
        let mut vm = Vm {
            globals: HashMap::new(),
            global_names: mc.global_names.clone(),
            structs: mc.struct_defs.clone(),
            protos: mc.protos.clone(),
            builtin_table: mc.builtin_names.iter().map(|n| builtins()[n]).collect(),
            frames: Vec::new(),
            stack: Vec::new(),
            builtin_site: None,
        };
        // 模块函数注册为全局闭包(空环境;函数体内模块级引用走 LoadGlobal)
        for item in &program.module.items {
            if let Item::Fn(f) = item {
                let Some(name) = &f.name else { continue };
                if let Some(proto) = mc.protos.iter().find(|p| p.name.as_deref() == Some(name.as_str())) {
                    let fv = Value::Fn(Rc::new(FnValue { def: proto.def.clone(), closure: Env::root() }));
                    vm.globals.insert(name.clone(), fv);
                }
            }
        }
        // init(let/顶层表达式)
        let init = Rc::new(mc.init.clone());
        let base = vm.stack.len();
        vm.stack.resize(base + init.nslots as usize, Value::Unit);
        let depth = vm.frames.len();
        vm.frames.push(Frame { proto: init, base, ip: 0, env: Env::root() });
        vm.run(depth)?;
        Ok(vm)
    }

    /// 运行 main;返回进程退出码。
    pub fn run_main(&mut self, argv: Vec<String>) -> Result<i32, Diagnostic> {
        let main = self.globals.get("main").cloned().ok_or_else(|| {
            Diagnostic::error(E5015, "no 'main' function found in the module".to_string(), Span::dummy())
                .with_hint("define (fn main () -> ... ...) or (fn main (args (Vec Str)) -> ... ...)")
        })?;
        let Value::Fn(fv) = main else {
            return Err(Diagnostic::error(
                E5015,
                format!("'main' is bound to {}, not a function", repr(&main)),
                Span::dummy(),
            )
            .with_hint("define main as a function"));
        };
        let result = match fv.def.params.len() {
            0 => self.call_fn_value(&fv, vec![], &dummy_call())?,
            1 => {
                let args = Value::Vec(Rc::new(argv.into_iter().map(Value::Str).collect()));
                self.call_fn_value(&fv, vec![args], &dummy_call())?
            }
            n => {
                return Err(Diagnostic::error(
                    E5005,
                    format!("main takes {} parameter(s); expected 0 or 1", n),
                    Span::dummy(),
                )
                .with_hint("main must be (fn main () -> ...) or (fn main (args (Vec Str)) -> ...)"));
            }
        };
        Ok(match result {
            Value::Int(i) => i as i32,
            _ => 0,
        })
    }

    /// 执行任意函数值(main 与内建回调共用入口)。
    pub fn call_fn_value(&mut self, fv: &Rc<FnValue>, args: Vec<Value>, call_site: &Expr) -> Result<Value, Diagnostic> {
        if fv.def.params.len() != args.len() {
            return Err(Diagnostic::error(
                E5005,
                format!(
                    "function '{}' takes {} argument(s), got {}",
                    fv.def.name.as_deref().unwrap_or("<lambda>"),
                    fv.def.params.len(),
                    args.len()
                ),
                call_site.span,
            )
            .with_node_id(call_site.node_id)
            .with_hint("match the declared parameter count"));
        }
        let proto = self.lookup_proto(fv)?;
        let base = self.stack.len();
        self.stack.extend(args);
        // 预留槽区(局部变量槽 + result 槽),未初始化槽为 Unit
        self.stack.resize(base + proto.nslots as usize, Value::Unit);
        let depth = self.frames.len();
        self.frames.push(Frame { proto, base, ip: 0, env: fv.closure.clone() });
        self.run(depth)
    }

    /// 执行至「depth 处的帧」返回:返回时栈顶为该帧的返回值。
    fn run(&mut self, depth: usize) -> Result<Value, Diagnostic> {
        loop {
            if self.frames.len() <= depth {
                // 目标帧已返回:栈顶是返回值
                return Ok(self.stack.last().cloned().unwrap_or(Value::Unit));
            }
            let (proto, base, ip) = {
                let f = self.frames.last().expect("frame underflow");
                (f.proto.clone(), f.base, f.ip)
            };
            if ip >= proto.chunk.code.len() {
                return Err(self.error_at(&proto, ip, E5005, "bytecode ran off the end".to_string()));
            }
            let (op, next_ip) = Op::decode(&proto.chunk.code, ip);
            self.frames.last_mut().unwrap().ip = next_ip;
            match op {
                Op::LoadConst(i) => {
                    let v = proto.chunk.consts[i as usize].clone();
                    self.stack.push(v);
                }
                Op::LoadLocal(s) => {
                    let v = self.stack[base + s as usize].clone();
                    self.stack.push(v);
                }
                Op::StoreLocal(s) => {
                    let v = self.stack.pop().expect("store underflow");
                    self.stack[base + s as usize] = v;
                }
                Op::LoadUpvalue(i) => {
                    let name = &proto.upvalues[i as usize].name;
                    let env = self.frames.last().unwrap().env.clone();
                    let v = env
                        .get(name)
                        .ok_or_else(|| self.error_at(&proto, ip, E5001, format!("unbound upvalue '{}'", name)))?;
                    self.stack.push(v);
                }
                Op::LoadGlobal(n) => {
                    let name = self.global_names[n as usize].clone();
                    let v = self
                        .globals
                        .get(&name)
                        .cloned()
                        .ok_or_else(|| self.error_at(&proto, ip, E5008, format!("unknown global '{}'", name)))?;
                    self.stack.push(v);
                }
                Op::DefineGlobal(n) => {
                    let v = self.stack.pop().expect("define underflow");
                    let name = self.global_names[n as usize].clone();
                    self.globals.insert(name, v);
                }
                Op::Closure(pi) => {
                    let proto2 = self.protos[pi as usize].clone();
                    let env = Env::root();
                    let mut self_refs = Vec::new();
                    for uv in proto2.upvalues.iter().rev() {
                        let v = self.stack.pop().expect("closure underflow");
                        if matches!(uv.loc, crate::opcode::UpvalueLoc::SelfRef) {
                            self_refs.push(uv.name.clone());
                        } else {
                            let _ = env.define(&uv.name, v);
                        }
                    }
                    let fv = Rc::new(FnValue { def: proto2.def.clone(), closure: env });
                    // 自引用:闭包自身写回环境(具名局部函数递归;Rc 环,进程级存活)
                    for name in self_refs {
                        let _ = fv.closure.define(&name, Value::Fn(fv.clone()));
                    }
                    self.stack.push(Value::Fn(fv));
                }
                Op::Call(argc) => {
                    let argc = argc as usize;
                    let callee_idx = self.stack.len() - 1 - argc;
                    let callee = self.stack.remove(callee_idx); // 仅移除闭包,实参左移原位保留
                    match callee {
                        Value::Fn(fv) => {
                            if fv.def.params.len() != argc {
                                return Err(self.error_at(
                                    &proto,
                                    ip,
                                    E5005,
                                    format!(
                                        "function '{}' takes {} argument(s), got {}",
                                        fv.def.name.as_deref().unwrap_or("<lambda>"),
                                        fv.def.params.len(),
                                        argc
                                    ),
                                ));
                            }
                            let base2 = callee_idx;
                            let proto2 = self.lookup_proto(&fv)?;
                            // 预留槽区
                            self.stack.resize(base2 + proto2.nslots as usize, Value::Unit);
                            self.frames.push(Frame { proto: proto2, base: base2, ip: 0, env: fv.closure.clone() });
                        }
                        other => {
                            return Err(self.error_at(&proto, ip, E5012, format!("calling non-function value {}", repr(&other))));
                        }
                    }
                }
                Op::CallBuiltin(bi, argc) => {
                    let argc = argc as usize;
                    let site = self.debug_at(&proto, ip).map(|d| (d.node_id, d.span));
                    self.builtin_site = site;
                    let args: Vec<Value> = self.stack.split_off(self.stack.len() - argc);
                    let builtin = self.builtin_table[bi as usize];
                    let dummy = dummy_expr_with(site);
                    let result = builtin(self, &args, &dummy)?;
                    self.stack.push(result);
                }
                Op::Return => {
                    let result = self.stack.pop().expect("return underflow");
                    let frame = self.frames.pop().expect("frame underflow");
                    self.stack.truncate(frame.base);
                    if self.frames.len() <= depth {
                        return Ok(result);
                    }
                    self.stack.push(result);
                }
                Op::Pop => {
                    self.stack.pop();
                }
                Op::Jump(off) => {
                    let f = self.frames.last_mut().unwrap();
                    f.ip = (f.ip as i32 + off as i32) as usize;
                }
                Op::JumpIfFalse(off) => {
                    let v = self.stack.pop().expect("jumpif underflow");
                    match v {
                        Value::Bool(true) => {}
                        Value::Bool(false) => {
                            let f = self.frames.last_mut().unwrap();
                            f.ip = (f.ip as i32 + off as i32) as usize;
                        }
                        other => {
                            return Err(self.error_at(&proto, ip, E5005, format!("condition must be Bool, got {}", repr(&other))));
                        }
                    }
                }
                Op::JumpIfTrue(off) => {
                    let v = self.stack.pop().expect("jumpif underflow");
                    match v {
                        Value::Bool(true) => {
                            let f = self.frames.last_mut().unwrap();
                            f.ip = (f.ip as i32 + off as i32) as usize;
                        }
                        Value::Bool(false) => {}
                        other => {
                            return Err(self.error_at(&proto, ip, E5005, format!("condition must be Bool, got {}", repr(&other))));
                        }
                    }
                }
                Op::VecNew(n) => {
                    let n = n as usize;
                    let items: Vec<Value> = self.stack.split_off(self.stack.len() - n);
                    self.stack.push(Value::Vec(Rc::new(items)));
                }
                Op::MapNew(n) => {
                    let n = n as usize;
                    let pairs: Vec<Value> = self.stack.split_off(self.stack.len() - 2 * n);
                    let mut map = std::collections::BTreeMap::new();
                    for i in 0..n {
                        let k = pairs[2 * i].clone();
                        let v = pairs[2 * i + 1].clone();
                        map.insert(k, v);
                    }
                    self.stack.push(Value::Map(Rc::new(map)));
                }
                Op::StructNew(si, n) => {
                    let n = n as usize;
                    let fields: Vec<Value> = self.stack.split_off(self.stack.len() - n);
                    let def = self.structs[si as usize].clone();
                    self.stack.push(Value::Struct { name: def.name.clone(), fields: Rc::new(fields) });
                }
                Op::RaisePre => {
                    return Err(self.raise(&proto, ip, E4001, "pre"));
                }
                Op::RaisePost => {
                    return Err(self.raise(&proto, ip, E4002, "post"));
                }
                Op::RaiseInvariant => {
                    return Err(self.raise(&proto, ip, E4003, "invariant"));
                }
                Op::HofFilter | Op::HofMap | Op::HofFold => {
                    let is_fold = matches!(op, Op::HofFold);
                    let vec = self.stack.pop().expect("hof underflow");
                    let init = if is_fold {
                        self.stack.pop().expect("hof underflow")
                    } else {
                        Value::Unit
                    };
                    let pred = self.stack.pop().expect("hof underflow");
                    let Value::Fn(pfv) = pred else {
                        return Err(self.error_at(&proto, ip, E5005, "filter/map/fold requires a function as first argument".to_string()));
                    };
                    let Value::Vec(items) = vec else {
                        return Err(self.error_at(&proto, ip, E5005, "filter/map/fold requires a Vec as last argument".to_string()));
                    };
                    let depth = self.frames.len();
                    let run_pred = |vm: &mut Vm, args: Vec<Value>| -> Result<Value, Diagnostic> {
                        if pfv.def.params.len() != args.len() {
                            return Err(vm.error_at(
                                &proto,
                                ip,
                                E5005,
                                format!(
                                    "predicate takes {} argument(s), got {}",
                                    pfv.def.params.len(),
                                    args.len()
                                ),
                            ));
                        }
                        let pproto = vm.lookup_proto(&pfv)?;
                        let base = vm.stack.len();
                        vm.stack.extend(args);
                        vm.stack.resize(base + pproto.nslots as usize, Value::Unit);
                        vm.frames.push(Frame { proto: pproto, base, ip: 0, env: pfv.closure.clone() });
                        vm.run(depth)
                    };
                    match op {
                        Op::HofFilter => {
                            let mut out = Vec::new();
                            for item in items.iter() {
                                match run_pred(self, vec![item.clone()])? {
                                    Value::Bool(true) => out.push(item.clone()),
                                    Value::Bool(false) => {}
                                    other => {
                                        return Err(self.error_at(
                                            &proto,
                                            ip,
                                            E5005,
                                            format!("filter predicate must return Bool, got {}", repr(&other)),
                                        ));
                                    }
                                }
                            }
                            self.stack.push(Value::Vec(Rc::new(out)));
                        }
                        Op::HofMap => {
                            let mut out = Vec::with_capacity(items.len());
                            for item in items.iter() {
                                out.push(run_pred(self, vec![item.clone()])?);
                            }
                            self.stack.push(Value::Vec(Rc::new(out)));
                        }
                        Op::HofFold => {
                            let mut acc = init;
                            for item in items.iter() {
                                acc = run_pred(self, vec![acc, item.clone()])?;
                            }
                            self.stack.push(acc);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }

    fn raise(&self, proto: &Prototype, ip: usize, code: &str, kind: &str) -> Diagnostic {
        let msg = match kind {
            "invariant" => format!(
                ":invariant violation (struct constructed in '{}')",
                proto.name.as_deref().unwrap_or("<lambda>")
            ),
            _ => format!(
                ":{} contract violated in function '{}'",
                kind,
                proto.name.as_deref().unwrap_or("<lambda>")
            ),
        };
        let (node_id, span) = self
            .debug_at(proto, ip)
            .map(|d| (Some(d.node_id), d.span))
            .unwrap_or((None, Span::dummy()));
        let d = Diagnostic::error(code, msg, span).with_hint("the constraint expression evaluated to false");
        match node_id {
            Some(id) => d.with_node_id(id),
            None => d,
        }
    }

    fn error_at(&self, proto: &Prototype, ip: usize, code: &str, message: String) -> Diagnostic {
        let (node_id, span) = self
            .debug_at(proto, ip)
            .map(|d| (Some(d.node_id), d.span))
            .unwrap_or((None, Span::dummy()));
        let d = Diagnostic::error(code, message, span);
        match node_id {
            Some(id) => d.with_node_id(id),
            None => d,
        }
    }

    fn debug_at(&self, proto: &Prototype, ip: usize) -> Option<DebugEntry> {
        let dbg = &proto.chunk.debug;
        let mut lo = 0usize;
        let mut hi = dbg.len();
        let mut best = None;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if dbg[mid].ip <= ip as u32 {
                best = Some(dbg[mid]);
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        best
    }

    fn lookup_proto(&self, fv: &Rc<FnValue>) -> Result<Rc<Prototype>, Diagnostic> {
        self.protos
            .iter()
            .find(|p| p.def.node_id == fv.def.node_id)
            .cloned()
            .ok_or_else(|| Diagnostic::error(E5008, "function prototype not found (VM/compiler mismatch)".to_string(), Span::dummy()))
    }
}

impl Host for Vm {
    /// 高阶内建回调:委托树遍历解释器执行(谓词为纯函数,语义一致;见模块注释)。
    fn call_fn_value(&self, f: &Rc<FnValue>, args: Vec<Value>, call_site: &Expr) -> Result<Value, Diagnostic> {
        Interp::new().call_fn_value(f, args, call_site)
    }

    fn struct_def(&self, name: &str) -> Option<Rc<StructDef>> {
        self.structs.iter().find(|s| s.name == name).cloned()
    }
}

fn dummy_call() -> Expr {
    Expr { node_id: NodeId(0), kind: ExprKind::Call { name: "main".to_string(), args: vec![] }, span: Span::dummy() }
}

fn dummy_expr_with(site: Option<(NodeId, Span)>) -> Expr {
    let (node_id, span) = site.unwrap_or((NodeId(0), Span::dummy()));
    Expr { node_id, kind: ExprKind::Call { name: String::new(), args: vec![] }, span }
}
