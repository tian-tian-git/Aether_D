//! 运行时值模型(02-semantics.md §2)与环境。
//!
//! - 全部值不可变,共享结构用 `Rc`;
//! - `Map` 键需要全序,故 `Value` 实现 `Ord`(NaN 确定性处理);
//! - `==` 为结构深相等;函数值按指针同一性比较。

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use aether_ast::printer::{escape_str, float_str, print_expr};
use aether_ast::{Expr, FnDef};

/// 运行时值。
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Vec(Rc<Vec<Value>>),
    Map(Rc<BTreeMap<Value, Value>>),
    Struct { name: String, fields: Rc<Vec<Value>> },
    Option(Option<Rc<Value>>),
    Result(Result<Rc<Value>, Rc<Value>>),
    Fn(Rc<FnValue>),
    Unit,
    /// `quote` 的产物(同像性)。
    Ast(Rc<Expr>),
}

/// 函数值 = 定义 + 词法捕获环境。
#[derive(Debug)]
pub struct FnValue {
    pub def: Rc<FnDef>,
    pub closure: Rc<Env>,
}

// ---------------------------------------------------------------------------
// 相等性与全序
// ---------------------------------------------------------------------------

fn float_eq(a: f64, b: f64) -> bool {
    if a.is_nan() || b.is_nan() {
        // Aether 的 == 是结构相等:NaN 与 NaN 相等(确定性;std 已守卫 NaN 的产生)。
        a.is_nan() && b.is_nan()
    } else {
        a == b
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        use Value::*;
        match (self, other) {
            (Int(a), Int(b)) => a == b,
            (Float(a), Float(b)) => float_eq(*a, *b),
            (Bool(a), Bool(b)) => a == b,
            (Str(a), Str(b)) => a == b,
            (Unit, Unit) => true,
            (Vec(a), Vec(b)) => a == b,
            (Map(a), Map(b)) => a == b,
            (Struct { name: na, fields: fa }, Struct { name: nb, fields: fb }) => na == nb && fa == fb,
            (Option(a), Option(b)) => a == b,
            (Result(a), Result(b)) => a == b,
            (Fn(a), Fn(b)) => Rc::ptr_eq(a, b),
            (Ast(a), Ast(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

fn float_cmp(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> Ordering {
        use Value::*;
        fn rank(v: &Value) -> u8 {
            match v {
                Int(_) => 0,
                Float(_) => 1,
                Bool(_) => 2,
                Str(_) => 3,
                Unit => 4,
                Vec(_) => 5,
                Map(_) => 6,
                Struct { .. } => 7,
                Option(_) => 8,
                Result(_) => 9,
                Fn(_) => 10,
                Ast(_) => 11,
            }
        }
        let (ra, rb) = (rank(self), rank(other));
        if ra != rb {
            return ra.cmp(&rb);
        }
        match (self, other) {
            (Int(a), Int(b)) => a.cmp(b),
            (Float(a), Float(b)) => float_cmp(*a, *b),
            (Bool(a), Bool(b)) => a.cmp(b),
            (Str(a), Str(b)) => a.cmp(b),
            (Vec(a), Vec(b)) => a.cmp(b),
            (Map(a), Map(b)) => a.iter().cmp(b.iter()),
            (Struct { name: na, fields: fa }, Struct { name: nb, fields: fb }) => (na, fa).cmp(&(nb, fb)),
            (Option(a), Option(b)) => a.cmp(b),
            (Result(a), Result(b)) => a.cmp(b),
            (Fn(a), Fn(b)) => Rc::as_ptr(a).cmp(&Rc::as_ptr(b)),
            (Ast(a), Ast(b)) => a.node_id.0.cmp(&b.node_id.0),
            _ => Ordering::Equal,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ---------------------------------------------------------------------------
// 规范表示(06-std.md §0)
// ---------------------------------------------------------------------------

/// 值规范表示。`out`/`err-out` 对 Str 例外输出裸字符串,由调用方处理;
/// 其余值一律使用本函数(可被解析器回读)。
pub fn repr(v: &Value) -> String {
    use Value as V;
    match v {
        V::Int(i) => i.to_string(),
        V::Float(f) => float_str(*f),
        V::Bool(b) => b.to_string(),
        V::Str(s) => escape_str(s),
        V::Unit => "()".to_string(),
        V::Vec(items) => {
            let body = items.iter().map(repr).collect::<Vec<_>>().join(" ");
            if body.is_empty() { "(vec)".to_string() } else { format!("(vec {})", body) }
        }
        V::Map(m) => {
            let body = m.iter().map(|(k, val)| format!("({} {})", repr(k), repr(val))).collect::<Vec<_>>().join(" ");
            if body.is_empty() { "(map-of)".to_string() } else { format!("(map-of {})", body) }
        }
        V::Struct { name, fields } => {
            let body = fields.iter().map(repr).collect::<Vec<_>>().join(" ");
            if body.is_empty() { format!("({})", name) } else { format!("({} {})", name, body) }
        }
        V::Option(Some(v)) => format!("(some {})", repr(v)),
        V::Option(None) => "none".to_string(),
        V::Result(Ok(v)) => format!("(ok {})", repr(v)),
        V::Result(Err(v)) => format!("(err {})", repr(v)),
        V::Fn(fv) => match &fv.def.name {
            Some(n) => format!("<fn {}>", n),
            None => "<lambda>".to_string(),
        },
        V::Ast(e) => print_expr(e),
    }
}

// ---------------------------------------------------------------------------
// 词法环境
// ---------------------------------------------------------------------------

/// 词法环境:变量表 + 父链。绑定不可变(无赋值语法),重复绑定在同一作用域非法。
#[derive(Debug, Default)]
pub struct Env {
    vars: RefCell<HashMap<String, Value>>,
    parent: Option<Rc<Env>>,
}

impl Env {
    pub fn root() -> Rc<Env> {
        Rc::new(Env::default())
    }

    pub fn child(parent: &Rc<Env>) -> Rc<Env> {
        Rc::new(Env { vars: RefCell::new(HashMap::new()), parent: Some(parent.clone()) })
    }

    /// 沿父链查找绑定。
    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.vars.borrow().get(name) {
            return Some(v.clone());
        }
        self.parent.as_ref().and_then(|p| p.get(name))
    }

    /// 在当前作用域定义绑定;同名重复定义返回 Err(调用方转 E5013)。
    pub fn define(&self, name: &str, value: Value) -> Result<(), ()> {
        let mut vars = self.vars.borrow_mut();
        if vars.contains_key(name) {
            return Err(());
        }
        vars.insert(name.to_string(), value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_equality() {
        assert_eq!(Value::Int(1), Value::Int(1));
        assert_eq!(Value::Vec(Rc::new(vec![Value::Int(1)])), Value::Vec(Rc::new(vec![Value::Int(1)])));
        assert_ne!(Value::Int(1), Value::Float(1.0));
        assert_eq!(Value::Option(None), Value::Option(None));
        let m1 = Value::Map(Rc::new(BTreeMap::from([(Value::Int(1), Value::Str("a".into()))])));
        let m2 = Value::Map(Rc::new(BTreeMap::from([(Value::Int(1), Value::Str("a".into()))])));
        assert_eq!(m1, m2);
    }

    #[test]
    fn ordering_ranks_by_variant_then_value() {
        assert!(Value::Int(2) > Value::Int(1));
        assert!(Value::Float(1.5) > Value::Int(1)); // Float 秩高于 Int
        assert!(Value::Bool(false) < Value::Str("".into()));
        assert_eq!(Value::Float(f64::NAN).cmp(&Value::Float(f64::NAN)), Ordering::Equal);
        assert!(Value::Float(f64::NAN) < Value::Float(0.0));
    }

    #[test]
    fn repr_forms() {
        assert_eq!(repr(&Value::Int(-7)), "-7");
        assert_eq!(repr(&Value::Float(3.0)), "3.0");
        assert_eq!(repr(&Value::Str("a\n".into())), "\"a\\n\"");
        assert_eq!(repr(&Value::Unit), "()");
        assert_eq!(repr(&Value::Vec(Rc::new(vec![Value::Int(1), Value::Int(2)]))), "(vec 1 2)");
        assert_eq!(repr(&Value::Vec(Rc::new(vec![]))), "(vec)");
        assert_eq!(repr(&Value::Option(None)), "none");
        assert_eq!(repr(&Value::Option(Some(Rc::new(Value::Int(1))))), "(some 1)");
        assert_eq!(repr(&Value::Result(Err(Rc::new(Value::Str("oops".into()))))), "(err \"oops\")");
        assert_eq!(
            repr(&Value::Struct { name: "Point".into(), fields: Rc::new(vec![Value::Float(1.0), Value::Float(2.0)]) }),
            "(Point 1.0 2.0)"
        );
    }

    #[test]
    fn env_scoping() {
        let root = Env::root();
        assert!(root.define("x", Value::Int(1)).is_ok());
        assert!(root.define("x", Value::Int(2)).is_err(), "same-scope redefinition must fail");
        let child = Env::child(&root);
        assert_eq!(child.get("x"), Some(Value::Int(1)));
        child.define("x", Value::Int(9)).unwrap(); // 遮蔽允许
        assert_eq!(child.get("x"), Some(Value::Int(9)));
        assert_eq!(root.get("x"), Some(Value::Int(1)));
        assert_eq!(child.get("missing"), None);
    }
}
