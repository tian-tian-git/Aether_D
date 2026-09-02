//! 契约静态验证(Z3,04-contracts.md)。
//!
//! v0.1 能力边界(诚实声明,详见 04-contracts.md):
//! - 支持:`Int`/`Bool`/`(Vec Int)` 值;算术/比较/布尔运算;`get`/`len`/`push`/`head`/`vec`/`range`;
//!   `if` 双分支符号执行;`let`;用户函数调用点的 `:pre`/`:post` 检查。
//! - 不支持(回退运行期检查):`Float`/`Str`/`Map`/`Option`/`Result`/结构体/`sorted?` 等。
//! - `Int` 按 Z3 无界整数处理(Aether 的环绕语义差异为已知限制);
//!   除法按 Z3 欧几里得语义(负数差异为已知限制)。
//!
//! 输出:可静态证实的契约/越界违反诊断(E4001/E4002/E5004,含反例模型)。

use std::collections::HashMap;
use std::ffi::CString;
use std::rc::Rc;

use aether_ast::{ContractKind, Expr, ExprKind, FnDef, Item, Program, TyKind};
use aether_diagnostic::Diagnostic;

use crate::z3ffi::Z3Api;

pub const E4001: &str = "E4001";
pub const E4002: &str = "E4002";
pub const E5004: &str = "E5004";
pub const E6001: &str = "E6001";
pub const E6002: &str = "E6002";

/// 尝试加载 Z3;不可用返回 None(调用方降级为「跳过静态验证」)。
pub fn try_load() -> Result<Z3Api, String> {
    unsafe { Z3Api::load() }
}

/// 静态验证整个程序,返回证实的违反诊断(空 = 无可证实的违反)。
pub fn verify_program(api: &Z3Api, program: &Program) -> Vec<Diagnostic> {
    let mut v = Verifier::new(api);
    v.run(program)
}

// 符号值
#[derive(Clone)]
enum SV {
    Int(*mut crate::z3ffi::Z3_ast),
    Bool(*mut crate::z3ffi::Z3_ast),
    Vec {
        arr: *mut crate::z3ffi::Z3_ast,
        len: *mut crate::z3ffi::Z3_ast,
        /// 长度是否可信(精确)。用户函数返回的新鲜 Vec 长度未知:
        /// 对其做越界检查会产生误报,故跳过(04-contracts 无假阳性承诺)。
        len_known: bool,
    },
}

struct Verifier<'a> {
    api: &'a Z3Api,
    ctx: *mut crate::z3ffi::Z3_context,
    solver: *mut crate::z3ffi::Z3_solver,
    fns: HashMap<String, Rc<FnDef>>,
    diagnostics: Vec<Diagnostic>,
    var_seq: u32,
}

impl<'a> Verifier<'a> {
    fn new(api: &'a Z3Api) -> Self {
        unsafe {
            let cfg = (api.Z3_mk_config)();
            let ctx = (api.Z3_mk_context)(cfg);
            let solver = (api.Z3_mk_solver)(ctx);
            (api.Z3_solver_inc_ref)(ctx, solver);
            Verifier { api, ctx, solver, fns: HashMap::new(), diagnostics: Vec::new(), var_seq: 0 }
        }
    }

    fn run(&mut self, program: &Program) -> Vec<Diagnostic> {
        for item in &program.module.items {
            if let Item::Fn(f) = item {
                if let Some(name) = &f.name {
                    self.fns.insert(name.clone(), Rc::new(f.clone()));
                }
            }
        }
        let fns: Vec<Rc<FnDef>> = self.fns.values().cloned().collect();
        for f in fns {
            self.verify_fn(&f);
        }
        self.diagnostics.clone()
    }

    fn fresh_name(&mut self, base: &str) -> String {
        let n = self.var_seq;
        self.var_seq += 1;
        format!("{}!{}", base, n)
    }

    fn mk_const(&mut self, name: &str, sort: *mut crate::z3ffi::Z3_sort) -> *mut crate::z3ffi::Z3_ast {
        unsafe {
            let cname = CString::new(name).unwrap();
            let sym = (self.api.Z3_mk_string_symbol)(self.ctx, cname.as_ptr());
            (self.api.Z3_mk_const)(self.ctx, sym, sort)
        }
    }

    fn int_sort(&mut self) -> *mut crate::z3ffi::Z3_sort {
        unsafe { (self.api.Z3_mk_int_sort)(self.ctx) }
    }

    fn bool_sort(&mut self) -> *mut crate::z3ffi::Z3_sort {
        unsafe { (self.api.Z3_mk_bool_sort)(self.ctx) }
    }

    fn int_lit(&mut self, v: i64) -> *mut crate::z3ffi::Z3_ast {
        unsafe {
            let s = CString::new(v.to_string()).unwrap();
            (self.api.Z3_mk_numeral)(self.ctx, s.as_ptr(), self.int_sort())
        }
    }

    fn push(&mut self) {
        unsafe { (self.api.Z3_solver_push)(self.ctx, self.solver) }
    }

    fn pop(&mut self) {
        unsafe { (self.api.Z3_solver_pop)(self.ctx, self.solver, 1) }
    }

    fn assert_ast(&mut self, a: *mut crate::z3ffi::Z3_ast) {
        unsafe { (self.api.Z3_solver_assert)(self.ctx, self.solver, a) }
    }

    /// 检查「当前条件 ∧ 该命题」是否可满足;返回 (是否 SAT, model 字符串)。
    fn check_sat(&mut self, prop: *mut crate::z3ffi::Z3_ast) -> (bool, String) {
        unsafe {
            self.push();
            self.assert_ast(prop);
            let r = (self.api.Z3_solver_check)(self.ctx, self.solver);
            let model = if r == 1 {
                let m = (self.api.Z3_solver_get_model)(self.ctx, self.solver);
                if m.is_null() {
                    String::new()
                } else {
                    let s = (self.api.Z3_model_to_string)(self.ctx, m);
                    if s.is_null() {
                        String::new()
                    } else {
                        std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
                    }
                }
            } else {
                String::new()
            };
            self.pop();
            (r == 1, model)
        }
    }

    fn not(&mut self, a: *mut crate::z3ffi::Z3_ast) -> *mut crate::z3ffi::Z3_ast {
        unsafe { (self.api.Z3_mk_not)(self.ctx, a) }
    }

    fn sv_type(&mut self, ty: &aether_ast::Ty, hint: &str) -> Option<SV> {
        match &ty.kind {
            TyKind::Int => {
                let name = self.fresh_name(hint);
                let sort = self.int_sort();
                Some(SV::Int(self.mk_const(&name, sort)))
            }
            TyKind::Bool => {
                let name = self.fresh_name(hint);
                let sort = self.bool_sort();
                Some(SV::Bool(self.mk_const(&name, sort)))
            }
            TyKind::VecOf(inner) if matches!(inner.kind, TyKind::Int) => {
                let arr_name = self.fresh_name(&format!("{}_arr", hint));
                let len_name = self.fresh_name(&format!("{}_len", hint));
                let int_sort = self.int_sort();
                let arr_sort = unsafe { (self.api.Z3_mk_array_sort)(self.ctx, int_sort, int_sort) };
                Some(SV::Vec {
                    arr: self.mk_const(&arr_name, arr_sort),
                    len: self.mk_const(&len_name, int_sort),
                    // 参数是「有约束的自由变量」:对其做越界检查是健全的(真阳性)
                    len_known: true,
                })
            }
            _ => None, // 不支持的类型:跳过静态
        }
    }

    fn verify_fn(&mut self, f: &Rc<FnDef>) {
        let mut env: HashMap<String, SV> = HashMap::new();
        let mut supported = true;
        for p in &f.params {
            match self.sv_type(&p.ty, &p.name) {
                Some(sv) => {
                    env.insert(p.name.clone(), sv);
                }
                None => supported = false,
            }
        }
        if !supported {
            return; // 参数含不支持类型:该函数整体跳过静态验证(运行期兜底)
        }
        self.push();
        // :pre 作为假设
        let mut pre_ok = true;
        for c in f.contracts.iter().filter(|c| c.kind == ContractKind::Pre) {
            match self.translate(&c.expr, &mut env) {
                Some(SV::Bool(a)) => self.assert_ast(a),
                Some(_) => {
                    pre_ok = false;
                }
                None => pre_ok = false,
            }
        }
        if !pre_ok {
            self.pop();
            return;
        }
        // 函数体符号执行
        let body = self.translate(&f.body, &mut env);
        // :post 检查(反例 = 证实的违反)
        for c in f.contracts.iter().filter(|c| c.kind == ContractKind::Post) {
            if let Some(body_sv) = &body {
                let mut post_env = env.clone();
                post_env.insert("result".to_string(), body_sv.clone());
                match self.translate(&c.expr, &mut post_env) {
                    Some(SV::Bool(p)) => {
                        let np = self.not(p);
                        let (sat, model) = self.check_sat(np);
                        if sat {
                            self.diagnostics.push(Self::violation(
                                E4002,
                                format!(
                                    ":post contract of '{}' can be violated ({})",
                                    f.name.as_deref().unwrap_or("<lambda>"),
                                    Self::model_note(&model)
                                ),
                                &c.expr,
                                "the post-condition does not follow from the function body",
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        self.pop();
    }

    /// 表达式 → 符号值;不支持返回 None(子树静默跳过,运行期兜底)。
    fn translate(&mut self, e: &Expr, env: &mut HashMap<String, SV>) -> Option<SV> {
        match &e.kind {
            ExprKind::Int(i) => Some(SV::Int(self.int_lit(*i))),
            ExprKind::Bool(b) => {
                let a = unsafe {
                    if *b {
                        (self.api.Z3_mk_true)(self.ctx)
                    } else {
                        (self.api.Z3_mk_false)(self.ctx)
                    }
                };
                Some(SV::Bool(a))
            }
            ExprKind::Var(name) => env.get(name).cloned(),
            ExprKind::If { cond, then_branch, else_branch } => {
                let c = self.translate(cond, env)?;
                let SV::Bool(cond_ast) = c else { return None };
                self.push();
                self.assert_ast(cond_ast);
                let t = self.translate(then_branch, env);
                self.pop();
                self.push();
                let ncond = self.not(cond_ast);
                self.assert_ast(ncond);
                let el = self.translate(else_branch, env);
                self.pop();
                match (t, el) {
                    (Some(SV::Int(t)), Some(SV::Int(e2))) => {
                        let ite = unsafe { (self.api.Z3_mk_ite)(self.ctx, cond_ast, t, e2) };
                        Some(SV::Int(ite))
                    }
                    (Some(SV::Bool(t)), Some(SV::Bool(e2))) => {
                        let ite = unsafe { (self.api.Z3_mk_ite)(self.ctx, cond_ast, t, e2) };
                        Some(SV::Bool(ite))
                    }
                    _ => None,
                }
            }
            ExprKind::Block(exprs) => {
                let mut last = None;
                for x in exprs {
                    last = self.translate(x, env);
                }
                last
            }
            ExprKind::VecLit(items) => {
                let mut svs = Vec::new();
                for it in items {
                    svs.push(self.translate(it, env));
                }
                if !svs.iter().all(Option::is_some) {
                    return None;
                }
                let ints: Vec<*mut crate::z3ffi::Z3_ast> =
                    svs.iter().filter_map(|s| match s { Some(SV::Int(a)) => Some(*a), _ => None }).collect();
                if ints.len() != items.len() {
                    return None;
                }
                let n = ints.len();
                let isort = self.int_sort();
                // 注意:Z3_mk_const_array 第一参数是「域类型」(元素类型),不是数组类型
                let base = if n == 0 {
                    let z = self.int_lit(0);
                    unsafe { (self.api.Z3_mk_const_array)(self.ctx, isort, z) }
                } else {
                    unsafe { (self.api.Z3_mk_const_array)(self.ctx, isort, ints[0]) }
                };
                let mut arr = base;
                for (i, v) in ints.iter().enumerate().skip(1) {
                    let idx = self.int_lit(i as i64);
                    arr = unsafe { (self.api.Z3_mk_store)(self.ctx, arr, idx, *v) };
                }
                Some(SV::Vec { arr, len: self.int_lit(n as i64), len_known: true })
            }
            ExprKind::Let(l) => {
                let v = self.translate(&l.value, env);
                if let Some(sv) = v {
                    env.insert(l.name.clone(), sv);
                }
                env.get(&l.name).cloned()
            }
            ExprKind::Call { name, args } => self.translate_call(name, args, e, env),
            _ => None,
        }
    }

    fn translate_call(
        &mut self,
        name: &str,
        args: &[Expr],
        call_expr: &Expr,
        env: &mut HashMap<String, SV>,
    ) -> Option<SV> {
        // 先翻译实参
        let mut arg_svs = Vec::new();
        for a in args {
            arg_svs.push(self.translate(a, env));
        }
        let all_some = arg_svs.iter().all(Option::is_some);
        let svs: Vec<SV> = arg_svs.iter().filter_map(|x| x.clone()).collect();

        // 用户函数调用:调用点检查 :pre,假设 :pre/:post
        if let Some(f) = self.fns.get(name).cloned() {
            if !all_some || svs.len() != f.params.len() {
                return None;
            }
            // :pre 检查
            for c in f.contracts.iter().filter(|c| c.kind == ContractKind::Pre) {
                let mut pre_env = HashMap::new();
                for (p, s) in f.params.iter().zip(&svs) {
                    pre_env.insert(p.name.clone(), s.clone());
                }
                // 同时带上外层环境(模块常量)
                for (k, v) in env.iter() {
                    pre_env.entry(k.clone()).or_insert_with(|| v.clone());
                }
                if let Some(SV::Bool(p)) = self.translate(&c.expr, &mut pre_env) {
                    let np = self.not(p);
                    let (sat, model) = self.check_sat(np);
                    if sat {
                        self.diagnostics.push(Self::violation(
                            E4001,
                            format!(
                                ":pre contract of '{}' can be violated at this call ({})",
                                name,
                                Self::model_note(&model)
                            ),
                            &c.expr,
                            "guard the arguments before calling",
                        ));
                    }
                    self.assert_ast(p); // 假设 pre 成立继续
                }
            }
            // 返回值:按声明类型生成新鲜符号值,并假设 :post
            let mut ret_sv = self.sv_type(&f.ret_ty, &format!("ret_{}", name));
            // 用户函数返回的 Vec:长度未建模(实际由函数行为决定)→ 跳过其越界检查(防误报)
            if let Some(SV::Vec { len_known, .. }) = ret_sv.as_mut() {
                *len_known = false;
            }
            if let (Some(ret_sv), Some(_)) = (&ret_sv, all_some.then_some(())) {
                for c in f.contracts.iter().filter(|c| c.kind == ContractKind::Post) {
                    let mut post_env = HashMap::new();
                    for (p, s) in f.params.iter().zip(&svs) {
                        post_env.insert(p.name.clone(), s.clone());
                    }
                    post_env.insert("result".to_string(), ret_sv.clone());
                    if let Some(SV::Bool(p)) = self.translate(&c.expr, &mut post_env) {
                        self.assert_ast(p);
                    }
                }
            }
            return ret_sv;
        }

        if !all_some {
            return None;
        }
        // 内建函数
        unsafe {
            match name {
                "+" | "-" | "*" | "min" | "max" => {
                    let ints: Vec<*mut crate::z3ffi::Z3_ast> =
                        svs.iter().filter_map(|s| match s { SV::Int(a) => Some(*a), _ => None }).collect();
                    if ints.len() != svs.len() || svs.is_empty() {
                        return None;
                    }
                    if name == "min" || name == "max" {
                        // min/max:ite 链
                        let mut acc = ints[0];
                        for x in &ints[1..] {
                            let cmp = if name == "min" {
                                (self.api.Z3_mk_lt)(self.ctx, *x, acc)
                            } else {
                                (self.api.Z3_mk_gt)(self.ctx, *x, acc)
                            };
                            acc = (self.api.Z3_mk_ite)(self.ctx, cmp, *x, acc);
                        }
                        return Some(SV::Int(acc));
                    }
                    let n = ints.len() as u32;
                    let ast = match name {
                        "+" => (self.api.Z3_mk_add)(self.ctx, n, ints.as_ptr()),
                        "-" => (self.api.Z3_mk_sub)(self.ctx, n, ints.as_ptr()),
                        _ => (self.api.Z3_mk_mul)(self.ctx, n, ints.as_ptr()),
                    };
                    Some(SV::Int(ast))
                }
                "/" | "%" => {
                    if svs.len() != 2 {
                        return None;
                    }
                    let (SV::Int(a), SV::Int(b)) = (&svs[0], &svs[1]) else { return None };
                    let ast = if name == "/" {
                        (self.api.Z3_mk_div)(self.ctx, *a, *b)
                    } else {
                        (self.api.Z3_mk_mod)(self.ctx, *a, *b)
                    };
                    Some(SV::Int(ast))
                }
                "abs" => {
                    if svs.len() != 1 {
                        return None;
                    }
                    let SV::Int(a) = &svs[0] else { return None };
                    let zero = self.int_lit(0);
                    let neg = (self.api.Z3_mk_sub)(self.ctx, 1, &zero);
                    let cmp = (self.api.Z3_mk_ge)(self.ctx, *a, zero);
                    let ite = (self.api.Z3_mk_ite)(self.ctx, cmp, *a, neg);
                    Some(SV::Int(ite))
                }
                "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                    if svs.len() != 2 {
                        return None;
                    }
                    let ast = match (&svs[0], &svs[1]) {
                        (SV::Int(a), SV::Int(b)) => match name {
                            "==" => (self.api.Z3_mk_eq)(self.ctx, *a, *b),
                            "!=" => (self.api.Z3_mk_not)(self.ctx, (self.api.Z3_mk_eq)(self.ctx, *a, *b)),
                            "<" => (self.api.Z3_mk_lt)(self.ctx, *a, *b),
                            "<=" => (self.api.Z3_mk_le)(self.ctx, *a, *b),
                            ">" => (self.api.Z3_mk_gt)(self.ctx, *a, *b),
                            _ => (self.api.Z3_mk_ge)(self.ctx, *a, *b),
                        },
                        (SV::Bool(a), SV::Bool(b)) if name == "==" || name == "!=" => {
                            let eq = (self.api.Z3_mk_eq)(self.ctx, *a, *b);
                            if name == "==" {
                                eq
                            } else {
                                (self.api.Z3_mk_not)(self.ctx, eq)
                            }
                        }
                        _ => return None,
                    };
                    Some(SV::Bool(ast))
                }
                "and" | "or" => {
                    let bools: Vec<*mut crate::z3ffi::Z3_ast> =
                        svs.iter().filter_map(|s| match s { SV::Bool(a) => Some(*a), _ => None }).collect();
                    if bools.len() != svs.len() {
                        return None;
                    }
                    if bools.is_empty() {
                        let ast = if name == "and" {
                            (self.api.Z3_mk_true)(self.ctx)
                        } else {
                            (self.api.Z3_mk_false)(self.ctx)
                        };
                        return Some(SV::Bool(ast));
                    }
                    let ast = if name == "and" {
                        (self.api.Z3_mk_and)(self.ctx, bools.len() as u32, bools.as_ptr())
                    } else {
                        (self.api.Z3_mk_or)(self.ctx, bools.len() as u32, bools.as_ptr())
                    };
                    Some(SV::Bool(ast))
                }
                "not" => {
                    let SV::Bool(a) = &svs[0] else { return None };
                    Some(SV::Bool((self.api.Z3_mk_not)(self.ctx, *a)))
                }
                "get" => {
                    if svs.len() != 2 {
                        return None;
                    }
                    let (SV::Vec { arr, len, len_known }, SV::Int(idx)) = (&svs[0], &svs[1]) else { return None };
                    // 长度未知的 Vec(用户函数返回值):越界检查会产生误报 → 跳过(无假阳性承诺)
                    if *len_known {
                        // 越界安全检查:sat(¬(0<=idx<len)) → 静态证实可越界
                        let zero = self.int_lit(0);
                        let lower = (self.api.Z3_mk_ge)(self.ctx, *idx, zero);
                        let upper = (self.api.Z3_mk_lt)(self.ctx, *idx, *len);
                        let in_bounds = (self.api.Z3_mk_and)(self.ctx, 2, [lower, upper].as_ptr());
                        let n_in = self.not(in_bounds);
                        let (sat, model) = self.check_sat(n_in);
                        if sat {
                            self.diagnostics.push(Self::violation(
                                E5004,
                                format!("index out of bounds is statically reachable ({})", Self::model_note(&model)),
                                call_expr,
                                "guard the index with 0 <= i < (len xs) before (get xs i)",
                            ));
                        }
                        self.assert_ast(in_bounds); // 假设安全继续
                    }
                    let sel = (self.api.Z3_mk_select)(self.ctx, *arr, *idx);
                    Some(SV::Int(sel))
                }
                "len" => match &svs[0] {
                    SV::Vec { len, .. } => Some(SV::Int(*len)),
                    _ => None,
                },
                "push" => {
                    if svs.len() != 2 {
                        return None;
                    }
                    let (SV::Vec { arr, len, len_known }, SV::Int(v)) = (&svs[0], &svs[1]) else { return None };
                    let arr2 = (self.api.Z3_mk_store)(self.ctx, *arr, *len, *v);
                    let one = self.int_lit(1);
                    let len2 = (self.api.Z3_mk_add)(self.ctx, 2, [*len, one].as_ptr());
                    Some(SV::Vec { arr: arr2, len: len2, len_known: *len_known })
                }
                "head" => {
                    let SV::Vec { arr, len, len_known } = &svs[0] else { return None };
                    let zero = self.int_lit(0);
                    if *len_known {
                        let nonempty = (self.api.Z3_mk_gt)(self.ctx, *len, zero);
                        let n_ne = self.not(nonempty);
                        let (sat, model) = self.check_sat(n_ne);
                        if sat {
                            self.diagnostics.push(Self::violation(
                                E5004,
                                format!("'head' on an empty Vec is statically reachable ({})", Self::model_note(&model)),
                                call_expr,
                                "guard with (empty? xs) before (head xs)",
                            ));
                        }
                        self.assert_ast(nonempty);
                    }
                    let sel = (self.api.Z3_mk_select)(self.ctx, *arr, zero);
                    Some(SV::Int(sel))
                }
                "vec" => {
                    let ints: Vec<*mut crate::z3ffi::Z3_ast> =
                        svs.iter().filter_map(|s| match s { SV::Int(a) => Some(*a), _ => None }).collect();
                    if ints.len() != svs.len() {
                        return None;
                    }
                    let n = ints.len();
                    let isort = self.int_sort();
                    let base = if n == 0 {
                        let zero = self.int_lit(0);
                        (self.api.Z3_mk_const_array)(self.ctx, isort, zero)
                    } else {
                        (self.api.Z3_mk_const_array)(self.ctx, isort, ints[0])
                    };
                    let mut arr = base;
                    for (i, v) in ints.iter().enumerate().skip(1) {
                        let idx = self.int_lit(i as i64);
                        arr = (self.api.Z3_mk_store)(self.ctx, arr, idx, *v);
                    }
                    Some(SV::Vec { arr, len: self.int_lit(n as i64), len_known: true })
                }
                "range" => {
                    if svs.len() != 2 {
                        return None;
                    }
                    let (SV::Int(lo), SV::Int(hi)) = (&svs[0], &svs[1]) else { return None };
                    // 长度 = max(hi-lo, 0);内容符号化
                    let zero = self.int_lit(0);
                    let diff = (self.api.Z3_mk_sub)(self.ctx, 2, [*hi, *lo].as_ptr());
                    let len = (self.api.Z3_mk_ite)(
                        self.ctx,
                        (self.api.Z3_mk_gt)(self.ctx, diff, zero),
                        diff,
                        zero,
                    );
                    let arr_sort = (self.api.Z3_mk_array_sort)(self.ctx, self.int_sort(), self.int_sort());
                    let name = self.fresh_name("range");
                    let arr = self.mk_const(&name, arr_sort);
                    Some(SV::Vec { arr, len, len_known: true })
                }
                "out" | "err-out" => Some(SV::Bool((self.api.Z3_mk_true)(self.ctx))), // 效果:占位,无断言
                _ => None,
            }
        }
    }

    fn violation(code: &str, message: String, e: &Expr, hint: &str) -> Diagnostic {
        Diagnostic::error(code, message, e.span)
            .with_node_id(e.node_id)
            .with_hint(hint)
    }

    /// 反例模型文案:模型为空 = 违反与输入无关(无条件成立)。
    fn model_note(model: &str) -> String {
        let m = model.trim();
        if m.is_empty() {
            "violation is unconditional".to_string()
        } else {
            format!("counterexample: {}", m)
        }
    }
}

impl Drop for Verifier<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.api.Z3_solver_dec_ref)(self.ctx, self.solver);
            (self.api.Z3_del_context)(self.ctx);
        }
    }
}
