//! 树遍历求值器(02-semantics.md)。
//!
//! 关键语义:
//! - 词法作用域 + 闭包;具名函数在其声明作用域内可见(含自身体内,支持递归);
//! - 契约运行时检查:进入函数前查 `:pre`,返回前查 `:post`(`result` 绑定返回值),
//!   结构体构造时查 `:invariant`;违反 → E4001/E4002/E4003;
//! - 一切失败均为结构化诊断(E5xxx,含节点定位)。

use std::collections::HashMap;
use std::rc::Rc;

use aether_ast::{ContractKind, Expr, ExprKind, Item, Program, StructDef};
use aether_diagnostic::Diagnostic;

use crate::builtins::{self, BuiltinFn};
use crate::value::{repr, Env, FnValue, Value};

pub const E4001: &str = "E4001";
pub const E4002: &str = "E4002";
pub const E4003: &str = "E4003";
pub const E5001: &str = "E5001";
pub const E5005: &str = "E5005";
pub const E5008: &str = "E5008";
pub const E5012: &str = "E5012";
pub const E5013: &str = "E5013";
pub const E5015: &str = "E5015";

fn err(code: &str, message: String, node: &Expr, hint: &str) -> Diagnostic {
    Diagnostic::error(code, message, node.span)
        .with_node_id(node.node_id)
        .with_hint(hint)
}

/// 解释器实例:内建注册表 + 结构体注册表。
pub struct Interp {
    builtins: HashMap<&'static str, BuiltinFn>,
    structs: std::cell::RefCell<HashMap<String, Rc<StructDef>>>,
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

impl Interp {
    pub fn new() -> Self {
        Interp { builtins: builtins::builtins(), structs: std::cell::RefCell::new(HashMap::new()) }
    }

    /// 按名字查结构体定义(供字段访问内建使用)。
    pub fn struct_def(&self, name: &str) -> Option<Rc<StructDef>> {
        self.structs.borrow().get(name).cloned()
    }

    /// 运行完整程序:建立模块环境 → 注册结构体与函数 → 依序求值顶层项 → 调用 `main`。
    /// 返回进程退出码(main 返回 Int 时即该值,否则 0)。无 `main` → E5015。
    pub fn run_program(&self, program: &Program, argv: Vec<String>) -> Result<i32, Diagnostic> {
        let v = self.eval_program_inner(program, argv, true)?;
        Ok(match v {
            Value::Int(i) => i as i32,
            _ => 0,
        })
    }

    /// REPL/库模块语义:求值整个程序,不要求 `main`;
    /// 有 `main` → 调用之(0 或 1 参)并返回其结果;
    /// 无 `main` → 依序求值顶层项,返回最后一个表达式的值(无则 Unit)。
    pub fn eval_program(&self, program: &Program, argv: Vec<String>) -> Result<Value, Diagnostic> {
        self.eval_program_inner(program, argv, false)
    }

    fn eval_program_inner(
        &self,
        program: &Program,
        argv: Vec<String>,
        require_main: bool,
    ) -> Result<Value, Diagnostic> {
        let module_env = Env::root();
        // 1. 注册结构体(先于求值,允许互相引用)
        for item in &program.module.items {
            if let Item::Struct(s) = item {
                if !self.structs.borrow().contains_key(&s.name) {
                    self.structs.borrow_mut().insert(s.name.clone(), Rc::new(s.clone()));
                }
            }
        }
        // 2. 注册具名函数(闭包捕获模块环境 → 支持递归)
        for item in &program.module.items {
            if let Item::Fn(f) = item {
                if let Some(name) = &f.name {
                    let fv = Value::Fn(Rc::new(FnValue { def: Rc::new(f.clone()), closure: module_env.clone() }));
                    let _ = module_env.define(name, fv); // 首定义优先(确定性)
                }
            }
        }
        // 3. 依序求值顶层项(let 绑定进模块环境;expr 求值并记录末值)
        let mut last_expr = Value::Unit;
        for item in &program.module.items {
            match item {
                Item::Let(l) => {
                    let v = self.eval(&l.value, &module_env)?;
                    let _ = module_env.define(&l.name, v);
                }
                Item::Expr(e) => {
                    last_expr = self.eval(e, &module_env)?;
                }
                _ => {}
            }
        }
        // 4. 调用 main(若存在)
        let Some(main) = module_env.get("main") else {
            if require_main {
                return Err(Diagnostic::error(
                    E5015,
                    "no 'main' function found in the module".to_string(),
                    program.module.span,
                )
                .with_hint("define (fn main () -> ... ...) or (fn main (args (Vec Str)) -> ... ...)"));
            }
            return Ok(last_expr);
        };
        let main_fn = match &main {
            Value::Fn(f) => f.clone(),
            other => {
                return Err(Diagnostic::error(
                    E5015,
                    format!("'main' is bound to {}, not a function", repr(&other)),
                    program.module.span,
                )
                .with_hint("define main as a function"));
            }
        };
        let args_value = Value::Vec(Rc::new(argv.into_iter().map(Value::Str).collect()));
        match main_fn.def.params.len() {
            0 => self.call_fn_value(&main_fn, vec![], &dummy_call()),
            1 => self.call_fn_value(&main_fn, vec![args_value], &dummy_call()),
            n => Err(Diagnostic::error(
                E5005,
                format!("main takes {} parameter(s); expected 0 or 1", n),
                program.module.span,
            )
            .with_hint("main must be (fn main () -> ...) or (fn main (args (Vec Str)) -> ...)")),
        }
    }

    /// 调用函数值(契约检查 + 求值)。builtins 的回调(filter/map/fold)也走这里。
    pub fn call_fn_value(&self, f: &Rc<FnValue>, args: Vec<Value>, call_site: &Expr) -> Result<Value, Diagnostic> {
        let def = &f.def;
        if args.len() != def.params.len() {
            return Err(Diagnostic::error(
                E5005,
                format!(
                    "function '{}' takes {} argument(s), got {}",
                    def.name.as_deref().unwrap_or("<lambda>"),
                    def.params.len(),
                    args.len()
                ),
                call_site.span,
            )
            .with_node_id(call_site.node_id)
            .with_hint("match the declared parameter count"));
        }
        let env = Env::child(&f.closure);
        for (p, a) in def.params.iter().zip(args) {
            let _ = env.define(&p.name, a);
        }
        // :pre 检查(进入函数前)
        for c in def.contracts.iter().filter(|c| c.kind == ContractKind::Pre) {
            match self.eval(&c.expr, &env)? {
                Value::Bool(true) => {}
                Value::Bool(false) => return Err(contract_violation(E4001, "pre", def, &c.expr)),
                other => {
                    return Err(err(
                        E4001,
                        format!(":pre contract must evaluate to Bool, got {}", repr(&other)),
                        &c.expr,
                        "contracts are boolean expressions",
                    ));
                }
            }
        }
        // 函数体
        let result = self.eval(&def.body, &env)?;
        // :post 检查(返回前,result 绑定返回值)
        let post_env = Env::child(&env);
        let _ = post_env.define("result", result.clone());
        for c in def.contracts.iter().filter(|c| c.kind == ContractKind::Post) {
            match self.eval(&c.expr, &post_env)? {
                Value::Bool(true) => {}
                Value::Bool(false) => return Err(contract_violation(E4002, "post", def, &c.expr)),
                other => {
                    return Err(err(
                        E4002,
                        format!(":post contract must evaluate to Bool, got {}", repr(&other)),
                        &c.expr,
                        "contracts are boolean expressions",
                    ));
                }
            }
        }
        Ok(result)
    }

    // -- 内部求值 --

    fn eval(&self, expr: &Expr, env: &Rc<Env>) -> Result<Value, Diagnostic> {
        match &expr.kind {
            ExprKind::Int(i) => Ok(Value::Int(*i)),
            ExprKind::Float(f) => Ok(Value::Float(*f)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Str(s) => Ok(Value::Str(s.clone())),
            ExprKind::Var(name) => env.get(name).ok_or_else(|| {
                err(E5001, format!("unbound variable '{}'", name), expr, "define it with (let ...) or as a parameter")
            }),
            ExprKind::Quote(inner) => Ok(Value::Ast(Rc::new(inner.as_ref().clone()))),
            ExprKind::If { cond, then_branch, else_branch } => match self.eval(cond, env)? {
                Value::Bool(true) => self.eval(then_branch, env),
                Value::Bool(false) => self.eval(else_branch, env),
                other => Err(err(E5005, format!("if condition must be Bool, got {}", repr(&other)), cond, "conditions are boolean expressions")),
            },
            ExprKind::Block(exprs) => {
                let block_env = Env::child(env);
                let mut last = Value::Unit;
                for e in exprs {
                    last = self.eval(e, &block_env)?;
                }
                Ok(last)
            }
            ExprKind::VecLit(items) => {
                let mut out = Vec::with_capacity(items.len());
                for e in items {
                    out.push(self.eval(e, env)?);
                }
                Ok(Value::Vec(Rc::new(out)))
            }
            ExprKind::MapLit(pairs) => {
                let mut out = std::collections::BTreeMap::new();
                for (k, v) in pairs {
                    let key = self.eval(k, env)?;
                    let value = self.eval(v, env)?;
                    out.insert(key, value);
                }
                Ok(Value::Map(Rc::new(out)))
            }
            ExprKind::Fn(f) => {
                let fv = Rc::new(FnValue { def: Rc::new(f.clone()), closure: env.clone() });
                if let Some(name) = &f.name {
                    if env.define(name, Value::Fn(fv.clone())).is_err() {
                        return Err(err(E5013, format!("duplicate binding '{}' in the same scope", name), expr, "rename one of the bindings"));
                    }
                }
                Ok(Value::Fn(fv))
            }
            ExprKind::Let(l) => {
                let v = self.eval(&l.value, env)?;
                if env.define(&l.name, v.clone()).is_err() {
                    return Err(err(E5013, format!("duplicate binding '{}' in the same scope", l.name), expr, "rename one of the bindings"));
                }
                Ok(v)
            }
            ExprKind::Call { name, args } => self.eval_call(name, args, expr, env),
        }
    }

    fn eval_call(&self, name: &str, args: &[Expr], call_expr: &Expr, env: &Rc<Env>) -> Result<Value, Diagnostic> {
        // 1. 内建函数(实参先求值:严格求值)
        if let Some(b) = self.builtins.get(name) {
            let mut arg_values = Vec::with_capacity(args.len());
            for a in args {
                arg_values.push(self.eval(a, env)?);
            }
            return b(self, &arg_values, call_expr);
        }
        // 2. 结构体构造器
        if let Some(def) = self.structs.borrow().get(name).cloned() {
            let mut arg_values = Vec::with_capacity(args.len());
            for a in args {
                arg_values.push(self.eval(a, env)?);
            }
            if arg_values.len() != def.fields.len() {
                return Err(err(
                    E5005,
                    format!("struct '{}' takes {} field(s), got {}", name, def.fields.len(), arg_values.len()),
                    call_expr,
                    "match the struct's field count",
                ));
            }
            let inv_env = Env::child(env);
            for (f, v) in def.fields.iter().zip(&arg_values) {
                let _ = inv_env.define(&f.name, v.clone());
            }
            for inv in &def.invariants {
                match self.eval(inv, &inv_env)? {
                    Value::Bool(true) => {}
                    Value::Bool(false) => {
                        return Err(err(
                            E4003,
                            format!(":invariant violation while constructing '{}'", name),
                            inv,
                            "the constructed value does not satisfy the struct's invariant",
                        ));
                    }
                    other => {
                        return Err(err(
                            E4003,
                            format!(":invariant must evaluate to Bool, got {}", repr(&other)),
                            inv,
                            "invariants are boolean expressions",
                        ));
                    }
                }
            }
            return Ok(Value::Struct { name: name.to_string(), fields: Rc::new(arg_values) });
        }
        // 3. 环境中的函数值
        if let Some(v) = env.get(name) {
            match v {
                Value::Fn(f) => {
                    let mut arg_values = Vec::with_capacity(args.len());
                    for a in args {
                        arg_values.push(self.eval(a, env)?);
                    }
                    return self.call_fn_value(&f, arg_values, call_expr);
                }
                other => {
                    return Err(err(
                        E5012,
                        format!("'{}' is bound to {}, not a function", name, repr(&other)),
                        call_expr,
                        "call only function values",
                    ));
                }
            }
        }
        Err(err(E5008, format!("unknown function '{}'", name), call_expr, "define it or check the spelling"))
    }
}

fn contract_violation(code: &str, kind: &str, def: &aether_ast::FnDef, cexpr: &Expr) -> Diagnostic {
    Diagnostic::error(
        code,
        format!(
            ":{} contract violated in function '{}'",
            kind,
            def.name.as_deref().unwrap_or("<lambda>")
        ),
        cexpr.span,
    )
    .with_node_id(cexpr.node_id)
    .with_hint("the constraint expression evaluated to false")
}

fn dummy_call() -> Expr {
    use aether_ast::{NodeId, Span};
    Expr {
        node_id: NodeId(0),
        kind: ExprKind::Call { name: "main".to_string(), args: vec![] },
        span: Span::dummy(),
    }
}
