//! AST → 字节码编译器(单遍,栈槽分配,词法上值)。
//!
//! 关键决策:
//! - 局部变量 = 栈槽;嵌套函数捕获 = 上值(Lua 式 Local/ParentUpvalue 链);
//! - 语言无赋值 → 上值无需可变单元,闭包创建时按名拷贝到词法 Env;
//! - 契约编译为内联字节码 + Raise* 指令;
//! - 模块级函数编译为全局闭包,模块级 let/表达式进入 init 原型。

use std::collections::HashMap;
use std::rc::Rc;

use aether_ast::{ContractKind, Expr, ExprKind, FnDef, Item, Program, StructDef};
use aether_interp::builtins::builtins;
use aether_interp::value::Value;

use crate::opcode::{Chunk, DebugEntry, ModuleCompiled, Op, Prototype, UpvalueLoc, UpvalueSrc};

pub fn compile(program: &Program) -> Result<ModuleCompiled, String> {
    Compiler::new().run(program)
}

struct Local {
    name: String,
    slot: u16,
}

struct FnCompiler {
    parent: Option<usize>,
    /// 本函数(具名局部函数)在父帧中的自引用槽;解析到该槽的上值标记 SelfRef。
    self_name_slot: Option<u16>,
    locals: Vec<Local>,
    scopes: Vec<usize>,
    upvalues: Vec<UpvalueSrc>,
    upvalue_names: HashMap<String, u8>,
    chunk: Chunk,
    nslots: u16,
    nparams: u8,
}

struct Compiler {
    builtin_names: Vec<&'static str>,
    builtin_map: HashMap<&'static str, u16>,
    global_names: Vec<String>,
    global_map: HashMap<String, u16>,
    structs: Vec<Rc<StructDef>>,
    struct_map: HashMap<String, u16>,
    fns: CompilerFns,
    protos: Vec<Rc<Prototype>>,
    proto_by_node: HashMap<u64, usize>,
}

struct CompilerFns {
    stack: Vec<FnCompiler>,
    current: usize,
}

impl Compiler {
    fn new() -> Self {
        let mut builtin_map = HashMap::new();
        let mut builtin_names = Vec::new();
        for (name, _) in builtins() {
            let idx = builtin_names.len() as u16;
            builtin_names.push(name);
            builtin_map.insert(name, idx);
        }
        Compiler {
            builtin_names,
            builtin_map,
            global_names: Vec::new(),
            global_map: HashMap::new(),
            structs: Vec::new(),
            struct_map: HashMap::new(),
            fns: CompilerFns { stack: Vec::new(), current: 0 },
            protos: Vec::new(),
            proto_by_node: HashMap::new(),
        }
    }

    fn run(mut self, program: &Program) -> Result<ModuleCompiled, String> {
        // 1. 结构体注册(按序)
        for item in &program.module.items {
            if let Item::Struct(s) = item {
                let idx = self.structs.len() as u16;
                self.structs.push(Rc::new(s.clone()));
                self.struct_map.insert(s.name.clone(), idx);
            }
        }
        // 2. 模块函数编译(闭包捕获空环境;名字入全局表)
        let mut main_idx = None;
        for item in &program.module.items {
            if let Item::Fn(f) = item {
                let proto = self.compile_prototype(f, None, None)?;
                let proto_idx = self.protos.len();
                if f.name.as_deref() == Some("main") {
                    main_idx = Some(proto_idx);
                }
                self.protos.push(Rc::new(proto));
                self.proto_by_node.insert(f.node_id.0, proto_idx);
                if let Some(name) = &f.name {
                    self.intern_global(name);
                }
            }
        }
        // 3. init 原型(顶层 let/表达式)
        let init = self.compile_init(program)?;
        Ok(ModuleCompiled {
            global_names: self.global_names,
            struct_defs: self.structs,
            builtin_names: self.builtin_names,
            protos: self.protos,
            init,
            main: main_idx,
        })
    }

    fn intern_global(&mut self, name: &str) -> u16 {
        if let Some(i) = self.global_map.get(name) {
            return *i;
        }
        let idx = self.global_names.len() as u16;
        self.global_names.push(name.to_string());
        self.global_map.insert(name.to_string(), idx);
        idx
    }

    fn compile_init(&mut self, program: &Program) -> Result<Prototype, String> {
        let fc = FnCompiler {
            parent: None,
            self_name_slot: None,
            locals: Vec::new(),
            scopes: vec![0],
            upvalues: Vec::new(),
            upvalue_names: HashMap::new(),
            chunk: Chunk { code: Vec::new(), consts: Vec::new(), debug: Vec::new() },
            nslots: 0,
            nparams: 0,
        };
        self.fns.stack.push(fc);
        self.fns.current = self.fns.stack.len() - 1;
        for item in &program.module.items {
            match item {
                Item::Let(l) => {
                    self.compile_expr(&l.value)?;
                    let g = self.intern_global(&l.name);
                    self.emit(Op::DefineGlobal(g));
                }
                Item::Expr(e) => {
                    self.compile_expr(e)?;
                    self.emit(Op::Pop);
                }
                _ => {}
            }
        }
        let unit = self.intern_const(Value::Unit);
        self.emit(Op::LoadConst(unit));
        self.emit(Op::Return);
        let fc = self.fns.stack.pop().unwrap();
        self.fns.current = 0;
        Ok(Prototype {
            name: None,
            def: Rc::new(FnDef {
                node_id: aether_ast::NodeId(0),
                name: None,
                params: vec![],
                ret_ty: aether_ast::Ty { kind: aether_ast::TyKind::Unit, span: aether_ast::Span::dummy() },
                is_effectful: false,
                contracts: vec![],
                body: Box::new(dummy_expr()),
                span: aether_ast::Span::dummy(),
            }),
            chunk: fc.chunk,
            nparams: 0,
            upvalues: fc.upvalues,
            nslots: fc.nslots,
        })
    }

    fn compile_prototype(&mut self, f: &FnDef, parent: Option<usize>, self_name_slot: Option<u16>) -> Result<Prototype, String> {
        // 挂起当前编译器
        let saved = self.fns.current;
        let mut fc = FnCompiler {
            parent,
            self_name_slot,
            locals: Vec::new(),
            scopes: vec![0],
            upvalues: Vec::new(),
            upvalue_names: HashMap::new(),
            chunk: Chunk { code: Vec::new(), consts: Vec::new(), debug: Vec::new() },
            nslots: 0,
            nparams: f.params.len() as u8,
        };
        // 参数槽 0..n
        for p in &f.params {
            fc.locals.push(Local { name: p.name.clone(), slot: fc.nslots });
            fc.nslots += 1;
        }
        let has_post = f.contracts.iter().any(|c| c.kind == ContractKind::Post);
        let result_slot = if has_post { Some(fc.nslots) } else { None };
        if result_slot.is_some() {
            fc.nslots += 1;
        }
        self.fns.stack.push(fc);
        self.fns.current = self.fns.stack.len() - 1;

        // :pre 契约 → 内联检查(真值跳过 Raise)
        for c in f.contracts.iter().filter(|c| c.kind == ContractKind::Pre) {
            self.compile_expr(&c.expr)?;
            let j = self.emit_jump_placeholder(Op::JumpIfTrue(0));
            self.debug(&c.expr);
            self.emit(Op::RaisePre);
            self.patch_jump(j);
        }
        // 函数体
        self.compile_expr(&f.body)?;
        if let Some(rslot) = result_slot {
            self.emit(Op::StoreLocal(rslot));
            // :post 契约 → 内联检查(result 槽)
            for c in f.contracts.iter().filter(|c| c.kind == ContractKind::Post) {
                self.emit(Op::LoadLocal(rslot));
                // 在 result 槽位置临时绑定名字 "result"
                let saved_locals = self.fns.stack[self.fns.current].locals.len();
                self.fns.stack[self.fns.current].locals.push(Local { name: "result".to_string(), slot: rslot });
                self.compile_expr(&c.expr)?;
                self.fns.stack[self.fns.current].locals.truncate(saved_locals);
                let j = self.emit_jump_placeholder(Op::JumpIfTrue(0));
                self.debug(&c.expr);
                self.emit(Op::RaisePost);
                self.patch_jump(j);
            }
            self.emit(Op::LoadLocal(rslot));
        }
        self.emit(Op::Return);

        let fc = self.fns.stack.pop().unwrap();
        self.fns.current = saved;
        Ok(Prototype {
            name: f.name.clone(),
            def: Rc::new(f.clone()),
            chunk: fc.chunk,
            nparams: fc.nparams,
            upvalues: fc.upvalues,
            nslots: fc.nslots,
        })
    }

    // -- 表达式编译 --

    fn compile_expr(&mut self, e: &Expr) -> Result<(), String> {
        self.debug(e);
        match &e.kind {
            ExprKind::Int(i) => {
                let idx = self.intern_const(Value::Int(*i));
                self.emit(Op::LoadConst(idx));
            }
            ExprKind::Float(f) => {
                let idx = self.intern_const(Value::Float(*f));
                self.emit(Op::LoadConst(idx));
            }
            ExprKind::Bool(b) => {
                let idx = self.intern_const(Value::Bool(*b));
                self.emit(Op::LoadConst(idx));
            }
            ExprKind::Str(s) => {
                let idx = self.intern_const(Value::Str(s.clone()));
                self.emit(Op::LoadConst(idx));
            }
            ExprKind::Var(name) => self.compile_var(e, name)?,
            ExprKind::Quote(inner) => {
                let idx = self.intern_const(Value::Ast(Rc::new(inner.as_ref().clone())));
                self.emit(Op::LoadConst(idx));
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                self.compile_expr(cond)?;
                let j1 = self.emit_jump_placeholder(Op::JumpIfFalse(0));
                self.compile_expr(then_branch)?;
                let j2 = self.emit_jump_placeholder(Op::Jump(0));
                self.patch_jump(j1);
                self.compile_expr(else_branch)?;
                self.patch_jump(j2);
            }
            ExprKind::Block(exprs) => {
                self.push_scope();
                for (i, x) in exprs.iter().enumerate() {
                    self.compile_expr(x)?;
                    if i + 1 < exprs.len() {
                        self.emit(Op::Pop);
                    }
                }
                if exprs.is_empty() {
                    let idx = self.intern_const(Value::Unit);
                    self.emit(Op::LoadConst(idx));
                }
                self.pop_scope();
            }
            ExprKind::VecLit(items) => {
                for x in items {
                    self.compile_expr(x)?;
                }
                self.emit(Op::VecNew(items.len() as u8));
            }
            ExprKind::MapLit(pairs) => {
                for (k, v) in pairs {
                    self.compile_expr(k)?;
                    self.compile_expr(v)?;
                }
                self.emit(Op::MapNew(pairs.len() as u8));
            }
            ExprKind::Fn(f) => self.compile_fn_expr(f)?,
            ExprKind::Let(l) => {
                self.compile_expr(&l.value)?;
                let slot = self.declare_local_slot(&l.name);
                self.emit(Op::StoreLocal(slot)); // 弹出值入槽
                self.emit(Op::LoadLocal(slot)); // let 表达式值留在栈上
            }
            ExprKind::Call { name, args } => self.compile_call(e, name, args)?,
        }
        Ok(())
    }

    fn compile_var(&mut self, e: &Expr, name: &str) -> Result<(), String> {
        if let Some(slot) = self.resolve_local(self.fns.current, name) {
            self.emit(Op::LoadLocal(slot));
            return Ok(());
        }
        if let Some(idx) = self.resolve_upvalue(self.fns.current, name) {
            self.emit(Op::LoadUpvalue(idx));
            return Ok(());
        }
        let g = self.intern_global(name);
        self.emit(Op::LoadGlobal(g));
        let _ = e;
        Ok(())
    }

    fn compile_fn_expr(&mut self, f: &FnDef) -> Result<(), String> {
        // 具名局部函数:先占槽(自递归可见)
        let name_slot = f.name.as_ref().map(|name| self.declare_local_slot(name));
        let parent = self.fns.current;
        let proto = self.compile_prototype(f, Some(parent), name_slot)?;
        let idx = self.protos.len();
        self.proto_by_node.insert(f.node_id.0, idx);
        self.protos.push(Rc::new(proto));
        // 上值按序压栈(从当前帧局部槽/上值读取;SelfRef 压 Unit 占位)
        for uv in &self.protos[idx].upvalues.clone() {
            match &uv.loc {
                UpvalueLoc::Local(slot) => {
                    self.emit(Op::LoadLocal(*slot));
                }
                UpvalueLoc::ParentUpvalue(i) => {
                    self.emit(Op::LoadUpvalue(*i));
                }
                UpvalueLoc::SelfRef => {
                    let unit = self.intern_const(Value::Unit);
                    self.emit(Op::LoadConst(unit));
                }
            }
        }
        self.emit(Op::Closure(idx as u16));
        if let Some(slot) = name_slot {
            self.emit(Op::StoreLocal(slot)); // 闭包入槽(自递归可见)
            self.emit(Op::LoadLocal(slot)); // 表达式值留在栈上
        }
        Ok(())
    }

    fn compile_call(&mut self, e: &Expr, name: &str, args: &[Expr]) -> Result<(), String> {
        if let Some(si) = self.struct_map.get(name).copied() {
            for a in args {
                self.compile_expr(a)?;
            }
            // 字段名临时槽 + 不变量内联检查
            let def = self.structs[si as usize].clone();
            self.push_scope();
            let field_slots: Vec<u16> = def
                .fields
                .iter()
                .map(|f| self.declare_local_slot(&f.name))
                .collect();
            let n = args.len() as u8;
            // 字段值在栈上顺序为 0..n-1(先编译的在底);StoreLocal 弹出栈顶 →
            // 按逆序绑定,使 field_i = arg_i
            for i in 0..field_slots.len() {
                let rev = field_slots.len() - 1 - i;
                self.emit(Op::StoreLocal(field_slots[rev]));
            }
            // 再按字段序压回栈供 StructNew 消费
            for slot in &field_slots {
                self.emit(Op::LoadLocal(*slot));
            }
            self.emit(Op::StructNew(si, n));
            for inv in &def.invariants {
                self.compile_expr(inv)?;
                let j = self.emit_jump_placeholder(Op::JumpIfTrue(0));
                self.debug(inv);
                self.emit(Op::RaiseInvariant);
                self.patch_jump(j);
            }
            self.pop_scope();
            return Ok(());
        }
        if let Some(bi) = self.builtin_map.get(name).copied() {
            for a in args {
                self.compile_expr(a)?;
            }
            // 高阶内建原生化:filter/map/fold 由 VM 内联执行谓词闭包
            let hof = match name {
                "filter" if args.len() == 2 => Some(Op::HofFilter),
                "map" if args.len() == 2 => Some(Op::HofMap),
                "fold" if args.len() == 3 => Some(Op::HofFold),
                _ => None,
            };
            if let Some(op) = hof {
                self.emit(op);
            } else {
                self.emit(Op::CallBuiltin(bi, args.len() as u8));
            }
            return Ok(());
        }
        // 其余:局部/上值/全局依次解析(模块函数、局部函数、闭包调用均覆盖)
        if let Some(slot) = self.resolve_local(self.fns.current, name) {
            self.emit(Op::LoadLocal(slot));
        } else if let Some(ui) = self.resolve_upvalue(self.fns.current, name) {
            self.emit(Op::LoadUpvalue(ui));
        } else {
            let g = self.intern_global(name);
            self.emit(Op::LoadGlobal(g));
        }
        for a in args {
            self.compile_expr(a)?;
        }
        self.emit(Op::Call(args.len() as u8));
        let _ = e;
        Ok(())
    }

    // -- 局部变量与上值 --

    fn declare_local_slot(&mut self, name: &str) -> u16 {
        let c = &mut self.fns.stack[self.fns.current];
        let slot = c.nslots;
        c.nslots += 1;
        c.locals.push(Local { name: name.to_string(), slot });
        slot
    }

    fn push_scope(&mut self) {
        let c = &mut self.fns.stack[self.fns.current];
        c.scopes.push(c.locals.len());
    }

    fn pop_scope(&mut self) {
        let c = &mut self.fns.stack[self.fns.current];
        let top = c.scopes.pop().unwrap_or(0);
        c.locals.truncate(top);
    }

    fn resolve_local(&self, cidx: usize, name: &str) -> Option<u16> {
        let c = &self.fns.stack[cidx];
        for l in c.locals.iter().rev() {
            if l.name == name {
                return Some(l.slot);
            }
        }
        None
    }

    /// 自 cidx 向上查找捕获;返回 cidx 帧内的上值索引。
    fn resolve_upvalue(&mut self, cidx: usize, name: &str) -> Option<u8> {
        let parent = self.fns.stack[cidx].parent?;
        // 父帧局部?
        if let Some(slot) = self.resolve_local(parent, name) {
            // 自引用:指向本函数自己的名字槽(具名局部函数递归)
            if self.fns.stack[cidx].self_name_slot == Some(slot) {
                return Some(self.add_upvalue(cidx, name, UpvalueLoc::SelfRef));
            }
            return Some(self.add_upvalue(cidx, name, UpvalueLoc::Local(slot)));
        }
        // 父帧的上值?
        let parent_up = self.resolve_upvalue(parent, name)?;
        Some(self.add_upvalue(cidx, name, UpvalueLoc::ParentUpvalue(parent_up)))
    }

    fn add_upvalue(&mut self, cidx: usize, name: &str, loc: UpvalueLoc) -> u8 {
        let c = &mut self.fns.stack[cidx];
        if let Some(i) = c.upvalue_names.get(name) {
            return *i;
        }
        let idx = c.upvalues.len() as u8;
        c.upvalues.push(UpvalueSrc { name: name.to_string(), loc });
        c.upvalue_names.insert(name.to_string(), idx);
        idx
    }

    // -- 发射与修补 --

    fn emit(&mut self, op: Op) -> usize {
        let c = &mut self.fns.stack[self.fns.current];
        let ip = c.chunk.code.len();
        op.encode(&mut c.chunk.code);
        ip
    }

    fn emit_jump_placeholder(&mut self, op: Op) -> usize {
        let c = &mut self.fns.stack[self.fns.current];
        let ip = c.chunk.code.len();
        op.encode(&mut c.chunk.code);
        ip + 1 // 偏移操作数起始
    }

    fn patch_jump(&mut self, operand_ip: usize) {
        let c = &mut self.fns.stack[self.fns.current];
        let target = c.chunk.code.len() as i16;
        // 偏移相对「下一条指令」:operand_ip + 2(操作数 2 字节)+ 2(操作数后无额外) —
        // 指令布局:[opcode 1B][operand 2B],解码后 ip = jump_ip + 5 = operand_ip + 4
        let offset = target - operand_ip as i16 - 4;
        c.chunk.code[operand_ip..operand_ip + 2].copy_from_slice(&offset.to_le_bytes());
    }

    fn intern_const(&mut self, v: Value) -> u16 {
        let c = &mut self.fns.stack[self.fns.current];
        if let Some(i) = c.chunk.consts.iter().position(|x| x == &v) {
            return i as u16;
        }
        let idx = c.chunk.consts.len() as u16;
        c.chunk.consts.push(v);
        idx
    }

    fn debug(&mut self, e: &Expr) {
        let c = &mut self.fns.stack[self.fns.current];
        let ip = c.chunk.code.len() as u32;
        c.chunk.debug.push(DebugEntry { ip, node_id: e.node_id, span: e.span });
    }
}

// init 原型的占位 def/expr
fn dummy_expr() -> Expr {
    Expr {
        node_id: aether_ast::NodeId(0),
        kind: ExprKind::Block(vec![]),
        span: aether_ast::Span::dummy(),
    }
}
