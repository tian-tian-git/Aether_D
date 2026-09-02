//! 内建函数(06-std.md)。
//!
//! 全部以普通调用形式出现;错误统一为结构化诊断 E5xxx(带调用节点定位)。
//! 除 `out`/`err-out` 外均为纯函数。
//! 宿主抽象 `Host` 允许树遍历解释器与字节码 VM 共享同一套内建实现。

use std::io::Write;
use std::rc::Rc;

use aether_ast::{Expr, StructDef};
use aether_diagnostic::Diagnostic;

use crate::value::{repr, FnValue, Value};

// 错误码(注册于 05-diagnostics.md)
pub const E5002: &str = "E5002";
pub const E5003: &str = "E5003";
pub const E5004: &str = "E5004";
pub const E5005: &str = "E5005";
pub const E5006: &str = "E5006";
pub const E5007: &str = "E5007";
pub const E5009: &str = "E5009";
pub const E5010: &str = "E5010";
pub const E5011: &str = "E5011";
pub const E5014: &str = "E5014";

/// range 的最大规模(资源守卫,保证确定性行为)。
pub const RANGE_LIMIT: i64 = 1_000_000;

/// 内建函数执行宿主:内建中的高阶操作(filter/map/fold 调用函数值、`.` 查结构体定义)
/// 需要回访宿主运行时。树遍历解释器与字节码 VM 各自实现。
pub trait Host {
    fn call_fn_value(&self, f: &Rc<FnValue>, args: Vec<Value>, call_site: &Expr) -> Result<Value, Diagnostic>;
    fn struct_def(&self, name: &str) -> Option<Rc<StructDef>>;
}

pub type BuiltinFn = fn(&dyn Host, &[Value], &Expr) -> Result<Value, Diagnostic>;

fn err(code: &str, message: String, node: &Expr, hint: &str) -> Diagnostic {
    Diagnostic::error(code, message, node.span)
        .with_node_id(node.node_id)
        .with_hint(hint)
}

fn arity(node: &Expr, name: &str, got: usize, expected: &str) -> Diagnostic {
    err(
        E5006,
        format!("'{}' got {} argument(s), expected {}", name, got, expected),
        node,
        &format!("call it as {}", expected),
    )
}

fn type_err(node: &Expr, name: &str, expected: &str, got: &Value) -> Diagnostic {
    err(
        E5005,
        format!("'{}' expected {}, got {}", name, expected, type_name(got)),
        node,
        "check the argument types",
    )
}

fn type_name(v: &Value) -> &'static str {
    use Value::*;
    match v {
        Int(_) => "Int",
        Float(_) => "Float",
        Bool(_) => "Bool",
        Str(_) => "Str",
        Vec(_) => "Vec",
        Map(_) => "Map",
        Struct { .. } => "Struct",
        Option(_) => "Option",
        Result(_) => "Result",
        Fn(_) => "Fn",
        Unit => "Unit",
        Ast(_) => "Ast",
    }
}

// ---------------------------------------------------------------------------
// 算术
// ---------------------------------------------------------------------------

fn numeric<'a>(node: &Expr, name: &str, args: &'a [Value]) -> Result<(&'a [Value], bool), Diagnostic> {
    // 返回 (参数, 是否全部为 Int);混合类型报错
    let all_int = args.iter().all(|a| matches!(a, Value::Int(_)));
    let all_float = args.iter().all(|a| matches!(a, Value::Float(_)));
    if !all_int && !all_float {
        return Err(err(
            E5005,
            format!("'{}' cannot mix Int and Float (no implicit conversion)", name),
            node,
            "convert explicitly before mixing numeric types",
        ));
    }
    Ok((args, all_int))
}

fn add(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    let (_, all_int) = numeric(node, "+", args)?;
    if all_int {
        Ok(Value::Int(args.iter().filter_map(|a| match a { Value::Int(i) => Some(i), _ => None }).fold(0i64, |acc, i| acc.wrapping_add(*i))))
    } else {
        Ok(Value::Float(args.iter().filter_map(|a| match a { Value::Float(f) => Some(f), _ => None }).fold(0.0, |acc, f| acc + f)))
    }
}

fn sub(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(arity(node, "-", 0, "1 or more"));
    }
    let (_, all_int) = numeric(node, "-", args)?;
    if all_int {
        let first = as_int(&args[0]);
        if args.len() == 1 {
            Ok(Value::Int(first.wrapping_neg()))
        } else {
            let mut acc = first;
            for a in &args[1..] {
                acc = acc.wrapping_sub(as_int(a));
            }
            Ok(Value::Int(acc))
        }
    } else {
        let first = as_float(&args[0]);
        if args.len() == 1 {
            Ok(Value::Float(-first))
        } else {
            let mut acc = first;
            for a in &args[1..] {
                acc -= as_float(a);
            }
            Ok(Value::Float(acc))
        }
    }
}

fn mul(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    let (_, all_int) = numeric(node, "*", args)?;
    if all_int {
        Ok(Value::Int(args.iter().filter_map(|a| match a { Value::Int(i) => Some(i), _ => None }).fold(1i64, |acc, i| acc.wrapping_mul(*i))))
    } else {
        Ok(Value::Float(args.iter().filter_map(|a| match a { Value::Float(f) => Some(f), _ => None }).fold(1.0, |acc, f| acc * f)))
    }
}

fn div(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, "/", args.len(), "2"));
    }
    let (_, all_int) = numeric(node, "/", args)?;
    if all_int {
        let (a, b) = (as_int(&args[0]), as_int(&args[1]));
        if b == 0 {
            return Err(err(E5002, "division by zero".to_string(), node, "guard the divisor before dividing"));
        }
        Ok(Value::Int(a.wrapping_div(b)))
    } else {
        let (a, b) = (as_float(&args[0]), as_float(&args[1]));
        if b == 0.0 {
            return Err(err(E5002, "division by zero".to_string(), node, "guard the divisor before dividing"));
        }
        Ok(Value::Float(a / b))
    }
}

fn rem(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, "%", args.len(), "2"));
    }
    let (a, b) = (expect_int(node, "%", &args[0])?, expect_int(node, "%", &args[1])?);
    if b == 0 {
        return Err(err(E5002, "division by zero".to_string(), node, "guard the divisor before taking the remainder"));
    }
    Ok(Value::Int(a.wrapping_rem(b)))
}

fn abs(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "abs", args.len(), "1"));
    }
    match &args[0] {
        Value::Int(i) => Ok(Value::Int(i.wrapping_abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        other => Err(type_err(node, "abs", "Int or Float", other)),
    }
}

fn min_max(node: &Expr, name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(arity(node, name, 0, "1 or more"));
    }
    let (_, all_int) = numeric(node, name, args)?;
    if all_int {
        let mut acc = as_int(&args[0]);
        for a in &args[1..] {
            acc = if name == "min" { acc.min(as_int(a)) } else { acc.max(as_int(a)) };
        }
        Ok(Value::Int(acc))
    } else {
        let mut acc = as_float(&args[0]);
        for a in &args[1..] {
            acc = if name == "min" { acc.min(as_float(a)) } else { acc.max(as_float(a)) };
        }
        Ok(Value::Float(acc))
    }
}

fn sqrt(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "sqrt", args.len(), "1"));
    }
    let f = expect_float(node, "sqrt", &args[0])?;
    if f < 0.0 {
        return Err(err(E5009, "sqrt domain error: negative argument".to_string(), node, "guard the argument to be >= 0"));
    }
    Ok(Value::Float(f.sqrt()))
}

fn as_int(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        _ => unreachable!("numeric() 已保证全 Int"),
    }
}

fn as_float(v: &Value) -> f64 {
    match v {
        Value::Float(f) => *f,
        _ => unreachable!("numeric() 已保证全 Float"),
    }
}

fn expect_int<'a>(node: &Expr, name: &str, v: &'a Value) -> Result<i64, Diagnostic> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(type_err(node, name, "Int", other)),
    }
}

fn expect_float<'a>(node: &Expr, name: &str, v: &'a Value) -> Result<f64, Diagnostic> {
    match v {
        Value::Float(f) => Ok(*f),
        other => Err(type_err(node, name, "Float", other)),
    }
}

fn expect_str<'a>(node: &Expr, name: &str, v: &'a Value) -> Result<&'a str, Diagnostic> {
    match v {
        Value::Str(s) => Ok(s),
        other => Err(type_err(node, name, "Str", other)),
    }
}

fn expect_vec<'a>(node: &Expr, name: &str, v: &'a Value) -> Result<Rc<Vec<Value>>, Diagnostic> {
    match v {
        Value::Vec(items) => Ok(items.clone()),
        other => Err(type_err(node, name, "Vec", other)),
    }
}

fn expect_fn<'a>(node: &Expr, name: &str, v: &'a Value) -> Result<Rc<FnValue>, Diagnostic> {
    match v {
        Value::Fn(f) => Ok(f.clone()),
        other => Err(type_err(node, name, "Fn", other)),
    }
}

// ---------------------------------------------------------------------------
// 比较与逻辑
// ---------------------------------------------------------------------------

fn eq_neq(node: &Expr, name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, name, args.len(), "2"));
    }
    let eq = args[0] == args[1];
    Ok(Value::Bool(if name == "==" { eq } else { !eq }))
}

fn ordered(node: &Expr, name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, name, args.len(), "2"));
    }
    use std::cmp::Ordering::*;
    let ord = match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).unwrap_or(Equal),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Str(a), Value::Str(b)) => a.cmp(b),
        (a, b) => {
            return Err(err(
                E5005,
                format!("'{}' is only defined for Int/Float/Bool/Str, got {} and {}", name, type_name(a), type_name(b)),
                node,
                "ordered comparison requires orderable types",
            ));
        }
    };
    let result = match name {
        "<" => ord == Less,
        "<=" => ord != Greater,
        ">" => ord == Greater,
        ">=" => ord != Less,
        _ => unreachable!(),
    };
    Ok(Value::Bool(result))
}

fn logic(node: &Expr, name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    for a in args {
        match a {
            Value::Bool(_) => {}
            other => return Err(type_err(node, name, "Bool arguments", other)),
        }
    }
    let is_and = name == "and";
    let mut acc = is_and;
    for a in args {
        let b = matches!(a, Value::Bool(true));
        acc = if is_and { acc && b } else { acc || b };
    }
    Ok(Value::Bool(acc))
}

fn not(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "not", args.len(), "1"));
    }
    match &args[0] {
        Value::Bool(b) => Ok(Value::Bool(!b)),
        other => Err(type_err(node, "not", "Bool", other)),
    }
}

// ---------------------------------------------------------------------------
// 谓词与集合
// ---------------------------------------------------------------------------

fn empty_p(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "empty?", args.len(), "1"));
    }
    Ok(Value::Bool(match &args[0] {
        Value::Vec(v) => v.is_empty(),
        Value::Map(m) => m.is_empty(),
        Value::Str(s) => s.is_empty(),
        other => return Err(type_err(node, "empty?", "Vec, Map or Str", other)),
    }))
}

fn finite_p(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "finite?", args.len(), "1"));
    }
    Ok(Value::Bool(expect_float(node, "finite?", &args[0])?.is_finite()))
}

fn sorted_p(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "sorted?", args.len(), "1"));
    }
    let items = expect_vec(node, "sorted?", &args[0])?;
    Ok(Value::Bool(items.windows(2).all(|w| w[0] <= w[1])))
}

fn permutation_p(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, "permutation?", args.len(), "2"));
    }
    let a = expect_vec(node, "permutation?", &args[0])?;
    let b = expect_vec(node, "permutation?", &args[1])?;
    let mut sa = a.as_ref().clone();
    let mut sb = b.as_ref().clone();
    sa.sort();
    sb.sort();
    Ok(Value::Bool(sa == sb))
}

fn len(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "len", args.len(), "1"));
    }
    Ok(Value::Int(match &args[0] {
        Value::Vec(v) => v.len() as i64,
        Value::Map(m) => m.len() as i64,
        Value::Str(s) => s.chars().count() as i64,
        other => return Err(type_err(node, "len", "Vec, Map or Str", other)),
    }))
}

fn head_tail(node: &Expr, name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, name, args.len(), "1"));
    }
    let items = expect_vec(node, name, &args[0])?;
    if items.is_empty() {
        return Err(err(E5003, format!("'{}' on an empty Vec", name), node, "guard with (empty? xs) before accessing"));
    }
    if name == "head" {
        Ok(items[0].clone())
    } else {
        Ok(Value::Vec(Rc::new(items[1..].to_vec())))
    }
}

fn get(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, "get", args.len(), "2"));
    }
    match &args[0] {
        Value::Vec(items) => {
            let idx = expect_int(node, "get", &args[1])?;
            if idx < 0 || idx as usize >= items.len() {
                return Err(err(E5004, format!("index {} out of bounds for Vec of length {}", idx, items.len()), node, "keep the index within [0, len)"));
            }
            Ok(items[idx as usize].clone())
        }
        Value::Map(m) => m.get(&args[1]).cloned().ok_or_else(|| {
            err(E5004, format!("key {} not found in Map", repr(&args[1])), node, "check with (has? m k) before (get m k)")
        }),
        other => Err(type_err(node, "get", "Vec or Map", other)),
    }
}

fn push(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, "push", args.len(), "2"));
    }
    let items = expect_vec(node, "push", &args[0])?;
    let mut new_items = items.as_ref().clone();
    new_items.push(args[1].clone());
    Ok(Value::Vec(Rc::new(new_items)))
}

fn concat(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    let all_vec = args.iter().all(|a| matches!(a, Value::Vec(_)));
    let all_str = args.iter().all(|a| matches!(a, Value::Str(_)));
    if args.is_empty() {
        return Ok(Value::Vec(Rc::new(vec![])));
    }
    if all_vec {
        let mut out = Vec::new();
        for a in args {
            if let Value::Vec(v) = a {
                out.extend(v.iter().cloned());
            }
        }
        Ok(Value::Vec(Rc::new(out)))
    } else if all_str {
        let mut out = String::new();
        for a in args {
            out.push_str(expect_str(node, "concat", a)?);
        }
        Ok(Value::Str(out))
    } else {
        Err(err(E5005, "concat expects all Vec or all Str".to_string(), node, "keep concatenated values the same type"))
    }
}

fn filter_map(host: &dyn Host, node: &Expr, name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, name, args.len(), "2"));
    }
    let f = expect_fn(node, name, &args[0])?;
    let items = expect_vec(node, name, &args[1])?;
    let mut out = Vec::new();
    for item in items.iter() {
        let r = host.call_fn_value(&f, vec![item.clone()], node)?;
        if name == "filter" {
            match r {
                Value::Bool(true) => out.push(item.clone()),
                Value::Bool(false) => {}
                other => return Err(type_err(node, "filter", "predicate returning Bool", &other)),
            }
        } else {
            out.push(r);
        }
    }
    Ok(Value::Vec(Rc::new(out)))
}

fn fold(host: &dyn Host, node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 3 {
        return Err(arity(node, "fold", args.len(), "3"));
    }
    let f = expect_fn(node, "fold", &args[0])?;
    let items = expect_vec(node, "fold", &args[2])?;
    let mut acc = args[1].clone();
    for item in items.iter() {
        acc = host.call_fn_value(&f, vec![acc, item.clone()], node)?;
    }
    Ok(acc)
}

fn range(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, "range", args.len(), "2"));
    }
    let lo = expect_int(node, "range", &args[0])?;
    let hi = expect_int(node, "range", &args[1])?;
    let size = hi.saturating_sub(lo);
    if size < 0 {
        return Ok(Value::Vec(Rc::new(vec![])));
    }
    if size > RANGE_LIMIT {
        return Err(err(
            E5014,
            format!("range of size {} exceeds the limit {}", size, RANGE_LIMIT),
            node,
            "keep ranges under 1000000 elements",
        ));
    }
    Ok(Value::Vec(Rc::new((lo..hi).map(Value::Int).collect())))
}

fn has_p(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, "has?", args.len(), "2"));
    }
    match &args[0] {
        Value::Map(m) => Ok(Value::Bool(m.contains_key(&args[1]))),
        other => Err(type_err(node, "has?", "Map", other)),
    }
}

fn keys(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "keys", args.len(), "1"));
    }
    match &args[0] {
        Value::Map(m) => Ok(Value::Vec(Rc::new(m.keys().cloned().collect()))),
        other => Err(type_err(node, "keys", "Map", other)),
    }
}

fn put(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 3 {
        return Err(arity(node, "put", args.len(), "3"));
    }
    match &args[0] {
        Value::Map(m) => {
            let mut new_map = m.as_ref().clone();
            new_map.insert(args[1].clone(), args[2].clone());
            Ok(Value::Map(Rc::new(new_map)))
        }
        other => Err(type_err(node, "put", "Map", other)),
    }
}

// ---------------------------------------------------------------------------
// 字符串与转换
// ---------------------------------------------------------------------------

fn str_len(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "str-len", args.len(), "1"));
    }
    Ok(Value::Int(expect_str(node, "str-len", &args[0])?.chars().count() as i64))
}

fn int_to_str(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "int->str", args.len(), "1"));
    }
    Ok(Value::Str(expect_int(node, "int->str", &args[0])?.to_string()))
}

fn str_to_int(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "str->int", args.len(), "1"));
    }
    let s = expect_str(node, "str->int", &args[0])?;
    let bytes = s.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.iter().enumerate().all(|(i, b)| b.is_ascii_digit() || (i == 0 && *b == b'-' && bytes.len() > 1));
    if !valid {
        return Err(err(E5010, format!("'{}' is not a decimal integer literal", s), node, "pass a string like \"42\" or \"-7\""));
    }
    match s.parse::<i64>() {
        Ok(i) => Ok(Value::Int(i)),
        Err(_) => Err(err(E5010, format!("'{}' is out of i64 range", s), node, "keep the integer within i64 range")),
    }
}

// ---------------------------------------------------------------------------
// Option / Result
// ---------------------------------------------------------------------------

fn option_construct(node: &Expr, name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    match name {
        "some" => {
            if args.len() != 1 {
                return Err(arity(node, name, args.len(), "1"));
            }
            Ok(Value::Option(Some(Rc::new(args[0].clone()))))
        }
        "none" => {
            if !args.is_empty() {
                return Err(arity(node, name, args.len(), "0"));
            }
            Ok(Value::Option(None))
        }
        "ok" => {
            if args.len() != 1 {
                return Err(arity(node, name, args.len(), "1"));
            }
            Ok(Value::Result(Ok(Rc::new(args[0].clone()))))
        }
        "err" => {
            if args.len() != 1 {
                return Err(arity(node, name, args.len(), "1"));
            }
            Ok(Value::Result(Err(Rc::new(args[0].clone()))))
        }
        _ => unreachable!(),
    }
}

fn option_pred(node: &Expr, name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, name, args.len(), "1"));
    }
    Ok(Value::Bool(match (&args[0], name) {
        (Value::Option(Some(_)), "is-some?") => true,
        (Value::Option(None), "is-none?") => true,
        (Value::Result(Ok(_)), "is-ok?") => true,
        (Value::Result(Err(_)), "is-err?") => true,
        (Value::Option(_), "is-some?" | "is-none?") => false,
        (Value::Result(_), "is-ok?" | "is-err?") => false,
        (other, _) => return Err(type_err(node, name, "Option or Result", other)),
    }))
}

fn unwrap(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "unwrap", args.len(), "1"));
    }
    match &args[0] {
        Value::Option(Some(v)) => Ok(v.as_ref().clone()),
        Value::Option(None) => Err(err(E5007, "unwrap on none".to_string(), node, "check with (is-some? x) before unwrapping")),
        Value::Result(Ok(v)) => Ok(v.as_ref().clone()),
        Value::Result(Err(e)) => Err(err(
            E5007,
            format!("unwrap on (err {})", repr(e)),
            node,
            "check with (is-ok? x) before unwrapping",
        )),
        other => Err(type_err(node, "unwrap", "Option or Result", other)),
    }
}

/// 字段访问:`(. obj "field")`。字段名是字符串(显式,无变量求值魔法)。
fn field_access(host: &dyn Host, node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 2 {
        return Err(arity(node, ".", args.len(), "2"));
    }
    let Value::Struct { name, fields } = &args[0] else {
        return Err(type_err(node, ".", "Struct", &args[0]));
    };
    let field = expect_str(node, ".", &args[1])?;
    let def = host.struct_def(name).ok_or_else(|| {
        err(E5011, format!("unknown struct '{}'", name), node, "define the struct before accessing its fields")
    })?;
    let idx = def.fields.iter().position(|f| f.name == field).ok_or_else(|| {
        err(
            E5011,
            format!("struct '{}' has no field '{}'", name, field),
            node,
            "use one of the struct's declared field names",
        )
    })?;
    Ok(fields[idx].clone())
}

// ---------------------------------------------------------------------------
// IO 与同像性
// ---------------------------------------------------------------------------

fn out(node: &Expr, args: &[Value], stderr: bool) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, if stderr { "err-out" } else { "out" }, args.len(), "1"));
    }
    let line = match &args[0] {
        Value::Str(s) => s.clone(), // Str 裸输出(06-std §0)
        other => repr(other),
    };
    let result = if stderr {
        writeln!(std::io::stderr().lock(), "{}", line)
    } else {
        writeln!(std::io::stdout().lock(), "{}", line)
    };
    result.map_err(|e| err(E5009, format!("IO error: {}", e), node, "check the output stream"))?;
    Ok(Value::Unit)
}

fn ast_to_str(node: &Expr, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() != 1 {
        return Err(arity(node, "ast->str", args.len(), "1"));
    }
    match &args[0] {
        Value::Ast(e) => Ok(Value::Str(aether_ast::printer::print_expr(e))),
        other => Err(type_err(node, "ast->str", "Ast", other)),
    }
}

// ---------------------------------------------------------------------------
// 注册表
// ---------------------------------------------------------------------------

/// 全部内建函数注册表。
pub fn builtins() -> std::collections::HashMap<&'static str, BuiltinFn> {
    use std::collections::HashMap;
    let mut m: HashMap<&'static str, BuiltinFn> = HashMap::new();
    m.insert("+", |_, a, n| add(n, a));
    m.insert("-", |_, a, n| sub(n, a));
    m.insert("*", |_, a, n| mul(n, a));
    m.insert("/", |_, a, n| div(n, a));
    m.insert("%", |_, a, n| rem(n, a));
    m.insert("abs", |_, a, n| abs(n, a));
    m.insert("min", |_, a, n| min_max(n, "min", a));
    m.insert("max", |_, a, n| min_max(n, "max", a));
    m.insert("sqrt", |_, a, n| sqrt(n, a));
    m.insert("==", |_, a, n| eq_neq(n, "==", a));
    m.insert("!=", |_, a, n| eq_neq(n, "!=", a));
    m.insert("<", |_, a, n| ordered(n, "<", a));
    m.insert("<=", |_, a, n| ordered(n, "<=", a));
    m.insert(">", |_, a, n| ordered(n, ">", a));
    m.insert(">=", |_, a, n| ordered(n, ">=", a));
    m.insert("and", |_, a, n| logic(n, "and", a));
    m.insert("or", |_, a, n| logic(n, "or", a));
    m.insert("not", |_, a, n| not(n, a));
    m.insert("empty?", |_, a, n| empty_p(n, a));
    m.insert("finite?", |_, a, n| finite_p(n, a));
    m.insert("sorted?", |_, a, n| sorted_p(n, a));
    m.insert("permutation?", |_, a, n| permutation_p(n, a));
    m.insert("len", |_, a, n| len(n, a));
    m.insert("head", |_, a, n| head_tail(n, "head", a));
    m.insert("tail", |_, a, n| head_tail(n, "tail", a));
    m.insert("get", |_, a, n| get(n, a));
    m.insert("push", |_, a, n| push(n, a));
    m.insert("concat", |_, a, n| concat(n, a));
    m.insert("filter", |i, a, n| filter_map(i, n, "filter", a));
    m.insert("map", |i, a, n| filter_map(i, n, "map", a));
    m.insert("fold", |i, a, n| fold(i, n, a));
    m.insert("range", |_, a, n| range(n, a));
    m.insert("has?", |_, a, n| has_p(n, a));
    m.insert("keys", |_, a, n| keys(n, a));
    m.insert("put", |_, a, n| put(n, a));
    m.insert("str-len", |_, a, n| str_len(n, a));
    m.insert("int->str", |_, a, n| int_to_str(n, a));
    m.insert("str->int", |_, a, n| str_to_int(n, a));
    m.insert("some", |_, a, n| option_construct(n, "some", a));
    m.insert("none", |_, a, n| option_construct(n, "none", a));
    m.insert("ok", |_, a, n| option_construct(n, "ok", a));
    m.insert("err", |_, a, n| option_construct(n, "err", a));
    m.insert("is-some?", |_, a, n| option_pred(n, "is-some?", a));
    m.insert("is-none?", |_, a, n| option_pred(n, "is-none?", a));
    m.insert("is-ok?", |_, a, n| option_pred(n, "is-ok?", a));
    m.insert("is-err?", |_, a, n| option_pred(n, "is-err?", a));
    m.insert("unwrap", |_, a, n| unwrap(n, a));
    m.insert(".", |i, a, n| field_access(i, n, a));
    m.insert("out", |_, a, n| out(n, a, false));
    m.insert("err-out", |_, a, n| out(n, a, true));
    m.insert("ast->str", |_, a, n| ast_to_str(n, a));
    m
}
