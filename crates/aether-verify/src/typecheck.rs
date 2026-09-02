//! 静态类型检查(03-types.md)。
//!
//! - 全显式标注,检查为主;仅 vec/map-of/if 分支/`none`/内建泛型处用类型变量推演;
//! - 用户函数单态;内建函数多态签名(§4 表)在调用点实例化;
//! - 错误码 E3001–E3008(注册于 05-diagnostics.md)。

use std::collections::HashMap;
use std::rc::Rc;

use aether_ast::{ContractKind, Expr, ExprKind, FnDef, Item, Program, Span, StructDef, Ty};
use aether_diagnostic::Diagnostic;

pub const E3001: &str = "E3001";
pub const E3002: &str = "E3002";
pub const E3003: &str = "E3003";
pub const E3004: &str = "E3004";
pub const E3005: &str = "E3005";
pub const E3006: &str = "E3006";
pub const E3007: &str = "E3007";
pub const E3008: &str = "E3008";

/// 检查整个程序;通过返回 Ok。
pub fn check_program(program: &Program) -> Result<(), Diagnostic> {
    TypeChecker::new().check(program)
}

/// 内部类型:AST 类型 + 类型变量。
#[derive(Debug, Clone, PartialEq)]
enum T {
    Unit,
    Int,
    Float,
    Bool,
    Str,
    Ast,
    Vec(Box<T>),
    Map(Box<T>, Box<T>),
    Option(Box<T>),
    Result(Box<T>, Box<T>),
    Fn(Vec<T>, Box<T>),
    Named(String),
    Var(u32),
}

struct TypeChecker {
    structs: HashMap<String, Rc<StructDef>>,
    fns: HashMap<String, (Vec<T>, T)>, // 模块级函数签名
    vars: Vec<Option<T>>,
    next_var: u32,
}

/// 作用域栈:同名遮蔽在嵌套作用域允许,同一作用域重复定义报 E3002。
#[derive(Clone)]
struct Env {
    scopes: Vec<HashMap<String, T>>,
}

impl Env {
    fn new() -> Self {
        Env { scopes: vec![HashMap::new()] }
    }

    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn get(&self, name: &str) -> Option<T> {
        for s in self.scopes.iter().rev() {
            if let Some(t) = s.get(name) {
                return Some(t.clone());
            }
        }
        None
    }

    /// 在当前作用域定义;同名已存在返回 Err(调用方转 E3002)。
    fn define(&mut self, name: &str, t: T) -> Result<(), ()> {
        let top = self.scopes.last_mut().expect("env always has at least one scope");
        if top.contains_key(name) {
            return Err(());
        }
        top.insert(name.to_string(), t);
        Ok(())
    }
}

impl TypeChecker {
    fn new() -> Self {
        TypeChecker { structs: HashMap::new(), fns: HashMap::new(), vars: Vec::new(), next_var: 0 }
    }

    fn check(&mut self, program: &Program) -> Result<(), Diagnostic> {
        // 1. 收集结构体
        for item in &program.module.items {
            if let Item::Struct(s) = item {
                if self.structs.contains_key(&s.name) {
                    return Err(Self::diag(E3002, format!("duplicate struct '{}'", s.name), s.span, Some(s.node_id), "rename one of the structs"));
                }
                self.structs.insert(s.name.clone(), Rc::new(s.clone()));
            }
        }
        // 2. 收集函数签名
        for item in &program.module.items {
            if let Item::Fn(f) = item {
                if let Some(name) = &f.name {
                    if self.fns.contains_key(name) {
                        return Err(Self::diag(E3002, format!("duplicate function '{}'", name), f.span, Some(f.node_id), "rename one of the functions"));
                    }
                    let params = f.params.iter().map(|p| self.from_ast_ty(&p.ty)).collect::<Result<Vec<_>, _>>()?;
                    let ret = self.from_ast_ty(&f.ret_ty)?;
                    self.fns.insert(name.clone(), (params, ret));
                }
            }
        }
        // 3. 模块级 let 与表达式(按声明序)
        let mut module_env = Env::new();
        let mut fn_items = Vec::new();
        for item in &program.module.items {
            match item {
                Item::Let(l) => {
                    let declared = self.from_ast_ty(&l.ty)?;
                    let actual = self.check_expr(&l.value, &mut module_env, Some(&declared))?;
                    let _ = actual;
                    if module_env.define(&l.name, declared).is_err() {
                        return Err(Self::diag(E3002, format!("duplicate binding '{}' in the same scope", l.name), l.span, Some(l.node_id), "rename one of the bindings"));
                    }
                }
                Item::Expr(e) => {
                    self.check_expr(e, &mut module_env, None)?;
                }
                Item::Fn(f) => fn_items.push(f.clone()),
                Item::Struct(_) => {}
            }
        }
        // 4. 函数体(静态按全模块 let 可见;运行时兜底 E5001)
        for f in &fn_items {
            self.check_fn(f, &module_env)?;
        }
        // 5. 结构体不变量(字段名遮蔽模块常量,如 (let W Int 20) 供 :invariant 引用)
        let structs: Vec<(String, Rc<StructDef>)> =
            self.structs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for (name, def) in structs {
            let mut inv_env = module_env.clone();
            inv_env.push();
            for f in &def.fields {
                let t = self.from_ast_ty(&f.ty)?;
                let _ = inv_env.define(&f.name, t);
            }
            for inv in &def.invariants {
                let t = self.check_expr(inv, &mut inv_env, None)?;
                self.require_bool(&t, inv, &format!(":invariant of struct '{}'", name))?;
            }
        }
        // 6. main 检查
        if let Some((params, ret)) = self.fns.get("main").cloned() {
            match params.len() {
                0 => {}
                1 => {
                    let expected = T::Vec(Box::new(T::Str));
                    match self.resolve(&params[0]) {
                        T::Vec(inner) if *inner == T::Str => {}
                        other => {
                            return Err(Self::diag(E3008, format!("main parameter must be (Vec Str), got {}", self.display(&other)), program.module.span, None, "main must be (fn main () -> ...) or (fn main (args (Vec Str)) -> ...)"));
                        }
                    }
                    let _ = expected;
                }
                n => {
                    return Err(Self::diag(E3008, format!("main takes {} parameter(s); expected 0 or 1", n), program.module.span, None, "main must be (fn main () -> ...) or (fn main (args (Vec Str)) -> ...)"));
                }
            }
            match self.resolve(&ret) {
                T::Unit | T::Int => {}
                other => {
                    return Err(Self::diag(E3008, format!("main must return Unit or Int, got {}", self.display(&other)), program.module.span, None, "return () or an Int exit code from main"));
                }
            }
        }
        Ok(())
    }

    fn check_fn(&mut self, f: &FnDef, module_env: &Env) -> Result<(), Diagnostic> {
        let mut env = module_env.clone();
        env.push(); // 参数作用域(参数允许遮蔽模块名)
        for p in &f.params {
            let t = self.from_ast_ty(&p.ty)?;
            if env.define(&p.name, t).is_err() {
                return Err(Self::diag(E3002, format!("duplicate parameter '{}'", p.name), p.span, Some(f.node_id), "rename one of the parameters"));
            }
        }
        let ret = self.from_ast_ty(&f.ret_ty)?;
        // :pre 契约 → Bool
        for c in f.contracts.iter().filter(|c| c.kind == ContractKind::Pre) {
            let t = self.check_expr(&c.expr, &mut env, None)?;
            self.require_bool(&t, &c.expr, ":pre contract")?;
        }
        let body_t = self.check_expr(&f.body, &mut env, Some(&ret))?;
        let _ = body_t;
        // :post 契约 → Bool(result 类型 = ret;result 在独立作用域,允许遮蔽参数)
        for c in f.contracts.iter().filter(|c| c.kind == ContractKind::Post) {
            env.push();
            let _ = env.define("result", ret.clone());
            let t = self.check_expr(&c.expr, &mut env, None)?;
            env.pop();
            self.require_bool(&t, &c.expr, ":post contract")?;
        }
        Ok(())
    }

    fn check_expr(&mut self, e: &Expr, env: &mut Env, expected: Option<&T>) -> Result<T, Diagnostic> {
        let t = match &e.kind {
            ExprKind::Int(_) => T::Int,
            ExprKind::Float(_) => T::Float,
            ExprKind::Bool(_) => T::Bool,
            ExprKind::Str(_) => T::Str,
            ExprKind::Var(name) => match env.get(name) {
                Some(t) => t,
                None => {
                    if let Some((params, ret)) = self.fns.get(name).cloned() {
                        T::Fn(params, Box::new(ret))
                    } else {
                        return Err(Self::diag(E3003, format!("undefined name '{}'", name), e.span, Some(e.node_id), "define it with (let ...) or as a function"));
                    }
                }
            },
            ExprKind::Quote(_) => T::Ast,
            ExprKind::If { cond, then_branch, else_branch } => {
                let ct = self.check_expr(cond, env, None)?;
                self.require_bool(&ct, cond, "if condition")?;
                let tt = self.check_expr(then_branch, env, None)?;
                let et = self.check_expr(else_branch, env, None)?;
                self.unify(tt, et, e.span, Some(e.node_id), "if branches")?
            }
            ExprKind::Block(exprs) => {
                env.push();
                let mut last = T::Unit;
                let mut failure = None;
                for (i, x) in exprs.iter().enumerate() {
                    let expect = if i + 1 == exprs.len() { expected.cloned() } else { None };
                    match self.check_expr(x, env, expect.as_ref()) {
                        Ok(t) => last = t,
                        Err(d) => {
                            failure = Some(d);
                            break;
                        }
                    }
                }
                env.pop();
                if let Some(d) = failure {
                    return Err(d);
                }
                last
            }
            ExprKind::VecLit(items) => {
                let elem = self.fresh();
                for x in items {
                    let xt = self.check_expr(x, env, None)?;
                    let _ = self.unify(xt, elem.clone(), x.span, Some(x.node_id), "vec element")?;
                }
                T::Vec(Box::new(elem))
            }
            ExprKind::MapLit(pairs) => {
                let k = self.fresh();
                let v = self.fresh();
                for (kx, vx) in pairs {
                    let kt = self.check_expr(kx, env, None)?;
                    let vt = self.check_expr(vx, env, None)?;
                    let _ = self.unify(kt, k.clone(), kx.span, Some(kx.node_id), "map key")?;
                    let _ = self.unify(vt, v.clone(), vx.span, Some(vx.node_id), "map value")?;
                }
                T::Map(Box::new(k), Box::new(v))
            }
            ExprKind::Fn(f) => {
                // 局部具名函数:绑定后检查
                let fty = T::Fn(
                    f.params.iter().map(|p| self.from_ast_ty(&p.ty)).collect::<Result<Vec<_>, _>>()?,
                    Box::new(self.from_ast_ty(&f.ret_ty)?),
                );
                if let Some(name) = &f.name {
                    if env.define(name, fty.clone()).is_err() {
                        return Err(Self::diag(E3002, format!("duplicate binding '{}' in the same scope", name), e.span, Some(e.node_id), "rename one of the bindings"));
                    }
                }
                self.check_fn(f, env)?;
                fty
            }
            ExprKind::Let(l) => {
                let declared = self.from_ast_ty(&l.ty)?;
                if env.define(&l.name, declared.clone()).is_err() {
                    return Err(Self::diag(E3002, format!("duplicate binding '{}' in the same scope", l.name), e.span, Some(e.node_id), "rename one of the bindings"));
                }
                let vt = self.check_expr(&l.value, env, Some(&declared))?;
                let _ = vt;
                declared
            }
            ExprKind::Call { name, args } => {
                let mut arg_types = Vec::with_capacity(args.len());
                for a in args {
                    arg_types.push(self.check_expr(a, env, None)?);
                }
                // 0. 环境中的函数值(参数/局部闭包,如 all? 的 pred)
                if let Some(bound) = env.get(name) {
                    match self.resolve(&bound) {
                        T::Fn(params, ret) => {
                            if arg_types.len() != params.len() {
                                return Err(Self::diag(
                                    E3005,
                                    format!("function '{}' takes {} argument(s), got {}", name, params.len(), arg_types.len()),
                                    e.span,
                                    Some(e.node_id),
                                    "match the declared parameter count",
                                ));
                            }
                            for (a, p) in arg_types.iter().zip(&params) {
                                let _ = self.unify(a.clone(), p.clone(), e.span, Some(e.node_id), &format!("argument of '{}'", name))?;
                            }
                            return Ok(*ret);
                        }
                        T::Var(_) => {
                            return Err(Self::diag(
                                E3001,
                                format!("cannot call '{}': its type is not determined", name),
                                e.span,
                                Some(e.node_id),
                                "annotate the binding with a (Fn ...) type",
                            ));
                        }
                        other => {
                            return Err(Self::diag(
                                E3001,
                                format!("'{}' is bound to {}, not a function", name, self.display(&other)),
                                e.span,
                                Some(e.node_id),
                                "call only function values",
                            ));
                        }
                    }
                }
                // 1. 模块级函数
                let r = if let Some((params, ret)) = self.fns.get(name).cloned() {
                    if arg_types.len() != params.len() {
                        return Err(Self::diag(
                            E3005,
                            format!("function '{}' takes {} argument(s), got {}", name, params.len(), arg_types.len()),
                            e.span,
                            Some(e.node_id),
                            "match the declared parameter count",
                        ));
                    }
                    for (a, p) in arg_types.iter().zip(&params) {
                        let _ = self.unify(a.clone(), p.clone(), e.span, Some(e.node_id), &format!("argument of '{}'", name))?;
                    }
                    ret
                } else if let Some(def) = self.structs.get(name).cloned() {
                    if arg_types.len() != def.fields.len() {
                        return Err(Self::diag(
                            E3005,
                            format!("struct '{}' takes {} field(s), got {}", name, def.fields.len(), arg_types.len()),
                            e.span,
                            Some(e.node_id),
                            "match the struct's field count",
                        ));
                    }
                    for (a, f) in arg_types.iter().zip(&def.fields) {
                        let expected_f = self.from_ast_ty(&f.ty)?;
                        let _ = self.unify(a.clone(), expected_f, e.span, Some(e.node_id), &format!("field '{}' of '{}'", f.name, name))?;
                    }
                    T::Named(name.clone())
                } else {
                    self.check_builtin(name, args, &arg_types, e)?
                };
                r
            }
        };
        match expected {
            Some(exp) => self.unify(t, exp.clone(), e.span, Some(e.node_id), "expression"),
            None => Ok(t),
        }
    }

    fn check_builtin(&mut self, name: &str, args: &[Expr], arg_types: &[T], e: &Expr) -> Result<T, Diagnostic> {
        let arity = |_self: &Self, want: &str| -> Diagnostic {
            Self::diag(
                E3005,
                format!("'{}' takes {}, got {} argument(s)", name, want, arg_types.len()),
                e.span,
                Some(e.node_id),
                "check the builtin's arity",
            )
        };
        let mism = |self_: &Self, exp: T, got: &T, what: &str| -> Diagnostic {
            Self::diag(
                E3001,
                format!("'{}': expected {} for {}, got {}", name, self_.display(&exp), what, self_.display(got)),
                e.span,
                Some(e.node_id),
                "check the argument types",
            )
        };
        match name {
            "+" | "-" | "*" | "min" | "max" => {
                let t = self.fresh();
                for a in arg_types {
                    let _ = self.unify(a.clone(), t.clone(), e.span, Some(e.node_id), &format!("argument of '{}'", name))?;
                }
                match self.resolve(&t) {
                    T::Int => Ok(T::Int),
                    T::Float => Ok(T::Float),
                    _ => {
                        let exp = T::Int;
                        Err(mism(self, exp, &t, &format!("all arguments of '{}'", name)))
                    }
                }
            }
            "/" | "%" => {
                if arg_types.len() != 2 {
                    return Err(arity(self, "2"));
                }
                let t = self.fresh();
                for a in arg_types {
                    let _ = self.unify(a.clone(), t.clone(), e.span, Some(e.node_id), &format!("argument of '{}'", name))?;
                }
                if name == "%" {
                    match self.resolve(&t) {
                        T::Int => Ok(T::Int),
                        other => Err(mism(self, T::Int, &other, "arguments of '%'")),
                    }
                } else {
                    match self.resolve(&t) {
                        T::Int => Ok(T::Int),
                        T::Float => Ok(T::Float),
                        other => Err(mism(self, T::Int, &other, "arguments of '/'")),
                    }
                }
            }
            "abs" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                match self.resolve(&arg_types[0]) {
                    T::Int => Ok(T::Int),
                    T::Float => Ok(T::Float),
                    other => Err(mism(self, T::Int, &other, "the argument of 'abs'")),
                }
            }
            "sqrt" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let _ = self.unify(arg_types[0].clone(), T::Float, e.span, Some(e.node_id), "argument of 'sqrt'")?;
                Ok(T::Float)
            }
            "finite?" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let _ = self.unify(arg_types[0].clone(), T::Float, e.span, Some(e.node_id), "argument of 'finite?'")?;
                Ok(T::Bool)
            }
            "==" | "!=" => {
                if arg_types.len() != 2 {
                    return Err(arity(self, "2"));
                }
                let _ = self.unify(arg_types[0].clone(), arg_types[1].clone(), e.span, Some(e.node_id), "arguments of '=='")?;
                Ok(T::Bool)
            }
            "<" | "<=" | ">" | ">=" => {
                if arg_types.len() != 2 {
                    return Err(arity(self, "2"));
                }
                let t = self.unify(arg_types[0].clone(), arg_types[1].clone(), e.span, Some(e.node_id), "arguments of '<'")?;
                match self.resolve(&t) {
                    T::Int | T::Float | T::Bool | T::Str => Ok(T::Bool),
                    other => Err(mism(self, T::Int, &other, "ordered comparison")),
                }
            }
            "and" | "or" => {
                for a in arg_types {
                    let _ = self.unify(a.clone(), T::Bool, e.span, Some(e.node_id), &format!("argument of '{}'", name))?;
                }
                Ok(T::Bool)
            }
            "not" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let _ = self.unify(arg_types[0].clone(), T::Bool, e.span, Some(e.node_id), "argument of 'not'")?;
                Ok(T::Bool)
            }
            "empty?" | "len" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let ok = match self.resolve(&arg_types[0]) {
                    T::Vec(_) | T::Map(..) | T::Str => true,
                    _ => false,
                };
                if !ok {
                    let exp = { let f = self.fresh(); T::Vec(Box::new(f)) };
                    return Err(mism(self, exp, &arg_types[0], &format!("argument of '{}'", name)));
                }
                Ok(if name == "empty?" { T::Bool } else { T::Int })
            }
            "sorted?" | "permutation?" => {
                let want = if name == "sorted?" { 1 } else { 2 };
                if arg_types.len() != want {
                    return Err(arity(self, if want == 1 { "1" } else { "2" }));
                }
                let elem = self.fresh();
                for a in arg_types {
                    let _ = self.unify(a.clone(), T::Vec(Box::new(elem.clone())), e.span, Some(e.node_id), &format!("argument of '{}'", name))?;
                }
                Ok(T::Bool)
            }
            "head" | "tail" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let elem = self.fresh();
                let _ = self.unify(arg_types[0].clone(), T::Vec(Box::new(elem.clone())), e.span, Some(e.node_id), &format!("argument of '{}'", name))?;
                Ok(if name == "head" { elem } else { T::Vec(Box::new(elem)) })
            }
            "get" => {
                if arg_types.len() != 2 {
                    return Err(arity(self, "2"));
                }
                match self.resolve(&arg_types[0]) {
                    T::Vec(inner) => {
                        let _ = self.unify(arg_types[1].clone(), T::Int, e.span, Some(e.node_id), "index of 'get'")?;
                        Ok(*inner)
                    }
                    T::Map(k, v) => {
                        let _ = self.unify(arg_types[1].clone(), *k, e.span, Some(e.node_id), "key of 'get'")?;
                        Ok(*v)
                    }
                    other => {
                        let exp = { let f = self.fresh(); T::Vec(Box::new(f)) };
                        Err(mism(self, exp, &other, "the first argument of 'get'"))
                    }
                }
            }
            "push" => {
                if arg_types.len() != 2 {
                    return Err(arity(self, "2"));
                }
                let elem = self.fresh();
                let _ = self.unify(arg_types[0].clone(), T::Vec(Box::new(elem.clone())), e.span, Some(e.node_id), "first argument of 'push'")?;
                let _ = self.unify(arg_types[1].clone(), elem.clone(), e.span, Some(e.node_id), "second argument of 'push'")?;
                Ok(T::Vec(Box::new(elem)))
            }
            "concat" => {
                let elem = self.fresh();
                let mut is_str = false;
                for a in arg_types {
                    match self.resolve(a) {
                        T::Str => is_str = true,
                        t => {
                            let _ = self.unify(t, T::Vec(Box::new(elem.clone())), e.span, Some(e.node_id), "argument of 'concat'")?;
                        }
                    }
                }
                if is_str {
                    Ok(T::Str)
                } else {
                    Ok(T::Vec(Box::new(elem)))
                }
            }
            "filter" | "map" => {
                if arg_types.len() != 2 {
                    return Err(arity(self, "2"));
                }
                let elem = self.fresh();
                let ret = self.fresh();
                let pred_ret = if name == "filter" { T::Bool } else { ret.clone() };
                let _ = self.unify(arg_types[0].clone(), T::Fn(vec![elem.clone()], Box::new(pred_ret)), e.span, Some(e.node_id), &format!("predicate of '{}'", name))?;
                let _ = self.unify(arg_types[1].clone(), T::Vec(Box::new(elem.clone())), e.span, Some(e.node_id), &format!("second argument of '{}'", name))?;
                Ok(T::Vec(Box::new(ret)))
            }
            "fold" => {
                if arg_types.len() != 3 {
                    return Err(arity(self, "3"));
                }
                let acc = self.fresh();
                let elem = self.fresh();
                let _ = self.unify(arg_types[0].clone(), T::Fn(vec![acc.clone(), elem.clone()], Box::new(acc.clone())), e.span, Some(e.node_id), "function of 'fold'")?;
                let _ = self.unify(arg_types[1].clone(), acc.clone(), e.span, Some(e.node_id), "initial value of 'fold'")?;
                let _ = self.unify(arg_types[2].clone(), T::Vec(Box::new(elem)), e.span, Some(e.node_id), "third argument of 'fold'")?;
                Ok(acc)
            }
            "range" => {
                if arg_types.len() != 2 {
                    return Err(arity(self, "2"));
                }
                for a in arg_types {
                    let _ = self.unify(a.clone(), T::Int, e.span, Some(e.node_id), "argument of 'range'")?;
                }
                Ok(T::Vec(Box::new(T::Int)))
            }
            "has?" => {
                if arg_types.len() != 2 {
                    return Err(arity(self, "2"));
                }
                let (k, _v) = match self.resolve(&arg_types[0]) {
                    T::Map(k, v) => (*k, *v),
                    other => {
                        let exp = { let a = self.fresh(); let b = self.fresh(); T::Map(Box::new(a), Box::new(b)) };
                        return Err(mism(self, exp, &other, "first argument of 'has?'"));
                    }
                };
                let _ = self.unify(arg_types[1].clone(), k, e.span, Some(e.node_id), "key of 'has?'")?;
                Ok(T::Bool)
            }
            "keys" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                match self.resolve(&arg_types[0]) {
                    T::Map(k, _v) => Ok(T::Vec(Box::new(*k))),
                    other => {
                        let exp = { let a = self.fresh(); let b = self.fresh(); T::Map(Box::new(a), Box::new(b)) };
                        Err(mism(self, exp, &other, "argument of 'keys'"))
                    }
                }
            }
            "put" => {
                if arg_types.len() != 3 {
                    return Err(arity(self, "3"));
                }
                let (k, v) = match self.resolve(&arg_types[0]) {
                    T::Map(k, v) => (*k, *v),
                    other => {
                        let exp = { let a = self.fresh(); let b = self.fresh(); T::Map(Box::new(a), Box::new(b)) };
                        return Err(mism(self, exp, &other, "first argument of 'put'"));
                    }
                };
                let _ = self.unify(arg_types[1].clone(), k.clone(), e.span, Some(e.node_id), "key of 'put'")?;
                let _ = self.unify(arg_types[2].clone(), v.clone(), e.span, Some(e.node_id), "value of 'put'")?;
                Ok(T::Map(Box::new(k), Box::new(v)))
            }
            "str-len" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let _ = self.unify(arg_types[0].clone(), T::Str, e.span, Some(e.node_id), "argument of 'str-len'")?;
                Ok(T::Int)
            }
            "int->str" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let _ = self.unify(arg_types[0].clone(), T::Int, e.span, Some(e.node_id), "argument of 'int->str'")?;
                Ok(T::Str)
            }
            "str->int" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let _ = self.unify(arg_types[0].clone(), T::Str, e.span, Some(e.node_id), "argument of 'str->int'")?;
                Ok(T::Int)
            }
            "some" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                Ok(T::Option(Box::new(arg_types[0].clone())))
            }
            "none" => {
                if !arg_types.is_empty() {
                    return Err(arity(self, "0"));
                }
                let f = self.fresh();
                Ok(T::Option(Box::new(f)))
            }
            "ok" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let f = self.fresh();
                Ok(T::Result(Box::new(arg_types[0].clone()), Box::new(f)))
            }
            "err" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let f = self.fresh();
                Ok(T::Result(Box::new(f), Box::new(arg_types[0].clone())))
            }
            "is-some?" | "is-none?" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let f = self.fresh();
                let _ = self.unify(arg_types[0].clone(), T::Option(Box::new(f)), e.span, Some(e.node_id), &format!("argument of '{}'", name))?;
                Ok(T::Bool)
            }
            "is-ok?" | "is-err?" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let a = self.fresh();
                let b = self.fresh();
                let _ = self.unify(arg_types[0].clone(), T::Result(Box::new(a), Box::new(b)), e.span, Some(e.node_id), &format!("argument of '{}'", name))?;
                Ok(T::Bool)
            }
            "unwrap" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                match self.resolve(&arg_types[0]) {
                    T::Option(inner) => Ok(*inner),
                    T::Result(inner, _e) => Ok(*inner),
                    other => {
                        let exp = { let f = self.fresh(); T::Option(Box::new(f)) };
                        Err(mism(self, exp, &other, "argument of 'unwrap'"))
                    }
                }
            }
            "." => {
                if arg_types.len() != 2 {
                    return Err(arity(self, "2"));
                }
                let field = match args.get(1).map(|a| &a.kind) {
                    Some(ExprKind::Str(s)) => s.clone(),
                    _ => {
                        return Err(Self::diag(E3006, "field name in (. obj \"field\") must be a string literal".to_string(), e.span, Some(e.node_id), "write the field name as a string literal"));
                    }
                };
                let sname = match self.resolve(&arg_types[0]) {
                    T::Named(n) => n,
                    other => return Err(mism(self, T::Named("<struct>".to_string()), &other, "first argument of '.'")),
                };
                let def = self.structs.get(&sname).cloned().ok_or_else(|| {
                    Self::diag(E3004, format!("unknown struct '{}'", sname), e.span, Some(e.node_id), "define the struct before accessing its fields")
                })?;
                match def.fields.iter().find(|f| f.name == field) {
                    Some(f) => self.from_ast_ty(&f.ty),
                    None => Err(Self::diag(E3006, format!("struct '{}' has no field '{}'", sname, field), e.span, Some(e.node_id), "use one of the struct's declared field names")),
                }
            }
            "out" | "err-out" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                Ok(T::Unit)
            }
            "ast->str" => {
                if arg_types.len() != 1 {
                    return Err(arity(self, "1"));
                }
                let _ = self.unify(arg_types[0].clone(), T::Ast, e.span, Some(e.node_id), "argument of 'ast->str'")?;
                Ok(T::Str)
            }
            _ => Err(Self::diag(E3003, format!("unknown function '{}'", name), e.span, Some(e.node_id), "define it or check the spelling")),
        }
    }
    // -- 工具 --

    fn require_bool(&self, t: &T, e: &Expr, what: &str) -> Result<(), Diagnostic> {
        match self.resolve(t) {
            T::Bool => Ok(()),
            other => Err(Self::diag(E3007, format!("{} must be Bool, got {}", what, self.display(&other)), e.span, Some(e.node_id), "use a boolean expression")),
        }
    }

    fn from_ast_ty(&mut self, ty: &Ty) -> Result<T, Diagnostic> {
        use aether_ast::TyKind;
        Ok(match &ty.kind {
            TyKind::Unit => T::Unit,
            TyKind::Int => T::Int,
            TyKind::Float => T::Float,
            TyKind::Bool => T::Bool,
            TyKind::Str => T::Str,
            TyKind::Ast => T::Ast,
            TyKind::VecOf(inner) => T::Vec(Box::new(self.from_ast_ty(inner)?)),
            TyKind::MapOf(k, v) => T::Map(Box::new(self.from_ast_ty(k)?), Box::new(self.from_ast_ty(v)?)),
            TyKind::Optional(inner) => T::Option(Box::new(self.from_ast_ty(inner)?)),
            TyKind::ResultOf(ok, err) => T::Result(Box::new(self.from_ast_ty(ok)?), Box::new(self.from_ast_ty(err)?)),
            TyKind::Fn { params, ret } => {
                let ps = params.iter().map(|p| self.from_ast_ty(p)).collect::<Result<Vec<_>, _>>()?;
                T::Fn(ps, Box::new(self.from_ast_ty(ret)?))
            }
            TyKind::Named(n) => {
                if !self.structs.contains_key(n) {
                    return Err(Self::diag(E3004, format!("unknown type '{}'", n), ty.span, None, "define a struct with this name, or use a built-in type"));
                }
                T::Named(n.clone())
            }
        })
    }

    fn fresh(&mut self) -> T {
        let v = T::Var(self.next_var);
        self.next_var += 1;
        self.vars.push(None);
        v
    }

    fn resolve(&self, t: &T) -> T {
        let mut cur = t.clone();
        loop {
            match cur {
                T::Var(i) => match self.vars.get(i as usize).cloned().flatten() {
                    Some(next) => cur = next,
                    None => return T::Var(i),
                },
                other => return other,
            }
        }
    }

    fn unify(&mut self, actual: T, expected: T, span: Span, node_id: Option<aether_ast::NodeId>, what: &str) -> Result<T, Diagnostic> {
        let actual = self.resolve(&actual);
        let expected = self.resolve(&expected);
        match (actual.clone(), expected.clone()) {
            (T::Var(i), t) => {
                // occurs check
                if occurs(&i, &t, &self.vars) {
                    return Err(Self::diag(E3001, format!("type mismatch in {}: infinite type", what), span, node_id, "check the expression"));
                }
                self.vars[i as usize] = Some(t.clone());
                Ok(t)
            }
            (t, T::Var(i)) => {
                if occurs(&i, &t, &self.vars) {
                    return Err(Self::diag(E3001, format!("type mismatch in {}: infinite type", what), span, node_id, "check the expression"));
                }
                self.vars[i as usize] = Some(t.clone());
                Ok(t)
            }
            (T::Vec(a), T::Vec(b)) => {
                let _ = self.unify(*a, *b, span, node_id, what)?;
                Ok(expected)
            }
            (T::Map(ak, av), T::Map(bk, bv)) => {
                let _ = self.unify(*ak, *bk, span, node_id, what)?;
                let _ = self.unify(*av, *bv, span, node_id, what)?;
                Ok(expected)
            }
            (T::Option(a), T::Option(b)) => {
                let _ = self.unify(*a, *b, span, node_id, what)?;
                Ok(expected)
            }
            (T::Result(a1, a2), T::Result(b1, b2)) => {
                let _ = self.unify(*a1, *b1, span, node_id, what)?;
                let _ = self.unify(*a2, *b2, span, node_id, what)?;
                Ok(expected)
            }
            (T::Fn(ap, ar), T::Fn(bp, br)) => {
                if ap.len() != bp.len() {
                    return Err(Self::diag(E3001, format!("type mismatch in {}: function arity {} vs {}", what, ap.len(), bp.len()), span, node_id, "check the function types"));
                }
                for (a, b) in ap.iter().zip(&bp) {
                    let _ = self.unify(a.clone(), b.clone(), span, node_id, what)?;
                }
                let _ = self.unify(*ar, *br, span, node_id, what)?;
                Ok(expected)
            }
            (a, b) if a == b => Ok(expected),
            (a, b) => Err(Self::diag(
                E3001,
                format!("type mismatch in {}: expected {}, got {}", what, self.display(&b), self.display(&a)),
                span,
                node_id,
                "check the expression's type",
            )),
        }
    }

    fn display(&self, t: &T) -> String {
        let t = self.resolve(t);
        match t {
            T::Unit => "()".to_string(),
            T::Int => "Int".to_string(),
            T::Float => "Float".to_string(),
            T::Bool => "Bool".to_string(),
            T::Str => "Str".to_string(),
            T::Ast => "Ast".to_string(),
            T::Vec(inner) => format!("(Vec {})", self.display(&inner)),
            T::Map(k, v) => format!("(Map {} {})", self.display(&k), self.display(&v)),
            T::Option(inner) => format!("(Option {})", self.display(&inner)),
            T::Result(a, b) => format!("(Result {} {})", self.display(&a), self.display(&b)),
            T::Fn(params, ret) => {
                let ps = params.iter().map(|p| self.display(p)).collect::<Vec<_>>().join(" ");
                format!("(Fn ({}) -> {})", ps, self.display(&ret))
            }
            T::Named(n) => n,
            T::Var(i) => format!("?{}", i),
        }
    }

    fn diag(code: &str, message: String, span: Span, node_id: Option<aether_ast::NodeId>, hint: &str) -> Diagnostic {
        let d = Diagnostic::error(code, message, span).with_hint(hint);
        match node_id {
            Some(id) => d.with_node_id(id),
            None => d,
        }
    }
}

fn occurs(var: &u32, t: &T, vars: &[Option<T>]) -> bool {
    match t {
        T::Var(i) if i == var => true,
        T::Var(i) => vars.get(*i as usize).cloned().flatten().map(|b| occurs(var, &b, vars)).unwrap_or(false),
        T::Vec(a) | T::Option(a) => occurs(var, a, vars),
        T::Map(a, b) | T::Result(a, b) => occurs(var, a, vars) || occurs(var, b, vars),
        T::Fn(ps, r) => ps.iter().any(|p| occurs(var, p, vars)) || occurs(var, r, vars),
        _ => false,
    }
}
