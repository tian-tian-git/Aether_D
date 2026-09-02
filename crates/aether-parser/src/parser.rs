//! 递归下降解析器(规范:docs/spec/v0.1/01-syntax.md)。
//!
//! 设计约定:
//! - 首个语法错误即停,报告 E2xxx 诊断(含 span 与修复建议);
//! - 每个 AST 节点由模块内全局自增分配唯一 `NodeId`;
//! - 词法错误(E1xxx)转换为诊断后直接上抛。
//!
//! 错误码(注册于 05-diagnostics.md):
//! E2001 意外 token / E2002 未闭合括号 / E2003 多余的 ')' /
//! E2004 元数错误 / E2005 形式位置非法 / E2006 期望标识符 /
//! E2007 期望类型 / E2008 契约种类不匹配。

use aether_ast::{
    Contract, ContractKind, Expr, ExprKind, Field, FnDef, Item, LetDef, Module, NodeId, Param,
    Program, Span, StructDef, Ty, TyKind,
};
use aether_diagnostic::Diagnostic;

use crate::lexer::{lex, Token, TokenKind};

pub const E2001: &str = "E2001";
pub const E2002: &str = "E2002";
pub const E2003: &str = "E2003";
pub const E2004: &str = "E2004";
pub const E2005: &str = "E2005";
pub const E2006: &str = "E2006";
pub const E2007: &str = "E2007";
pub const E2008: &str = "E2008";

/// 解析 Aether 源码为 `Program`;失败返回结构化诊断。
pub fn parse(src: &str) -> Result<Program, Diagnostic> {
    let tokens = lex(src).map_err(|e| {
        Diagnostic::error(e.code, e.message, e.span).with_hint(e.hint)
    })?;
    let mut p = Parser { tokens, pos: 0, next_node_id: 1 };
    p.parse_program()
}

/// 形式(供程序与表达式两层共用)。
enum Form {
    Fn(FnDef),
    Struct(StructDef),
    Let(LetDef),
    Expr(Expr),
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    next_node_id: u64,
}

impl Parser {
    // -- token 流基础操作 --

    fn peek(&self) -> &Token {
        // 词法器保证末尾必有 Eof。
        &self.tokens[self.pos]
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        t
    }

    fn alloc(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    // -- 诊断构造 --

    fn err(&self, code: &str, message: String, span: Span, hint: &str) -> Diagnostic {
        Diagnostic::error(code, message, span).with_hint(hint)
    }

    fn err_here(&self, code: &str, message: String, hint: &str) -> Diagnostic {
        self.err(code, message, self.peek().span, hint)
    }

    // -- 程序 / 模块 --

    fn parse_program(&mut self) -> Result<Program, Diagnostic> {
        let start = self.peek().span;
        let module = if self.peek_is_lparen_ident("module") {
            let open = self.bump(); // (
            self.bump(); // module
            let name = self.expect_ident("module name")?;
            let items = self.parse_items()?;
            let close = self.expect_rparen(&open)?;
            Module {
                name: Some(name),
                items,
                span: Span { start: open.span.start, end: close.span.end },
            }
        } else {
            Module {
                name: None,
                items: self.parse_items()?,
                span: Span { start: start.start, end: self.peek().span.start },
            }
        };
        Ok(Program { module })
    }

    fn peek_is_lparen_ident(&self, name: &str) -> bool {
        matches!(self.peek().kind, TokenKind::LParen)
            && matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                Some(TokenKind::Ident(n)) if n == name
            )
    }

    fn parse_items(&mut self) -> Result<Vec<Item>, Diagnostic> {
        if matches!(self.peek().kind, TokenKind::RParen) {
            return Err(self.err_here(
                E2003,
                "unexpected ')'".to_string(),
                "remove the extra ')' or check the bracket balance",
            ));
        }
        let mut items = Vec::new();
        while !self.at_eof() && !matches!(self.peek().kind, TokenKind::RParen) {
            let item = match self.parse_form(true)? {
                Form::Fn(f) => Item::Fn(f),
                Form::Struct(s) => Item::Struct(s),
                Form::Let(l) => Item::Let(l),
                Form::Expr(e) => Item::Expr(e),
            };
            items.push(item);
        }
        Ok(items)
    }

    // -- 形式分发 --

    fn parse_form(&mut self, allow_items: bool) -> Result<Form, Diagnostic> {
        let open = self.expect_lparen()?;
        let head = match self.peek().kind.clone() {
            TokenKind::Ident(name) => name,
            TokenKind::Eof => {
                return Err(self.err_here(
                    E2001,
                    "unexpected end of input inside a form".to_string(),
                    "check that every '(' has a matching ')'",
                ));
            }
            _ => {
                return Err(self.err_here(
                    E2006,
                    "expected a form name (identifier) after '('".to_string(),
                    "a parenthesized form must start with a function or special-form name",
                ));
            }
        };
        match head.as_str() {
            "module" => Err(self.err_here(
                E2005,
                "module form is not allowed here".to_string(),
                "modules may only appear at the top of a file, and only once",
            )),
            "fn" => {
                self.bump();
                Ok(Form::Fn(self.parse_fn(open)?))
            }
            "struct" => {
                if !allow_items {
                    return Err(self.err_here(
                        E2005,
                        "struct definition is not allowed in expression position".to_string(),
                        "move the struct definition to the module level",
                    ));
                }
                self.bump();
                Ok(Form::Struct(self.parse_struct(open)?))
            }
            "let" => {
                self.bump();
                let l = self.parse_let(open)?;
                if allow_items {
                    Ok(Form::Let(l))
                } else {
                    // let 作为表达式(02-semantics §3:值为初值,绑定作用于所在块后续)
                    let node_id = l.node_id;
                    let span = l.span;
                    Ok(Form::Expr(Expr { node_id, kind: ExprKind::Let(l), span }))
                }
            }
            "if" => {
                self.bump();
                Ok(Form::Expr(self.parse_if(open)?))
            }
            "block" => {
                self.bump();
                Ok(Form::Expr(self.parse_block(open)?))
            }
            "vec" => {
                self.bump();
                Ok(Form::Expr(self.parse_vec_lit(open)?))
            }
            "map-of" => {
                self.bump();
                Ok(Form::Expr(self.parse_map_lit(open)?))
            }
            "quote" => {
                self.bump();
                Ok(Form::Expr(self.parse_quote(open)?))
            }
            _ => {
                self.bump();
                Ok(Form::Expr(self.parse_call(open, head)?))
            }
        }
    }

    // -- 各项形式 --

    fn parse_fn(&mut self, open: Token) -> Result<FnDef, Diagnostic> {
        let node_id = self.alloc();
        // 具名或匿名:匿名时首 token 是 '('
        let name = if matches!(self.peek().kind, TokenKind::LParen) {
            None
        } else {
            Some(self.expect_ident("function name")?)
        };
        let mut params = Vec::new();
        while matches!(self.peek().kind, TokenKind::LParen) {
            // () 空参数列表
            if matches!(self.tokens.get(self.pos + 1).map(|t| &t.kind), Some(TokenKind::RParen)) {
                self.bump();
                self.bump();
                break;
            }
            let p_open = self.bump();
            let pname = self.expect_ident("parameter name")?;
            let pty = self.parse_type()?;
            self.expect_rparen(&p_open)?;
            params.push(Param { name: pname, ty: pty, span: Span { start: p_open.span.start, end: self.peek().span.start } });
        }
        if !matches!(self.peek().kind, TokenKind::Arrow) {
            return Err(self.err_here(
                E2001,
                "expected '->' after the parameter list".to_string(),
                "fn forms must look like (fn name (param Type) ... -> RetType body)",
            ));
        }
        self.bump(); // ->
        let ret_ty = self.parse_type()?;
        let is_effectful = if matches!(self.peek().kind, TokenKind::Bang) {
            self.bump();
            true
        } else {
            false
        };
        let mut contracts = Vec::new();
        while matches!(self.peek().kind, TokenKind::Colon) {
            self.bump(); // :
            let kind = self.expect_ident("contract kind (:pre or :post)")?;
            let kind = match kind.as_str() {
                "pre" => ContractKind::Pre,
                "post" => ContractKind::Post,
                _ => {
                    return Err(self.err_here(
                        E2008,
                        format!("contract ':{}' is not allowed on a function", kind),
                        "functions only accept :pre and :post; use :invariant on structs",
                    ));
                }
            };
            let expr = self.parse_expr()?;
            let span = expr.span;
            contracts.push(Contract { kind, expr, span });
        }
        let body = self.parse_expr()?;
        let close = self.expect_rparen(&open)?;
        Ok(FnDef {
            node_id,
            name,
            params,
            ret_ty,
            is_effectful,
            contracts,
            body: Box::new(body),
            span: Span { start: open.span.start, end: close.span.end },
        })
    }

    fn parse_struct(&mut self, open: Token) -> Result<StructDef, Diagnostic> {
        let node_id = self.alloc();
        let name = self.expect_ident("struct name")?;
        let mut fields = Vec::new();
        while matches!(self.peek().kind, TokenKind::LParen) {
            let f_open = self.bump();
            let fname = self.expect_ident("field name")?;
            let fty = self.parse_type()?;
            self.expect_rparen(&f_open)?;
            fields.push(Field { name: fname, ty: fty, span: Span { start: f_open.span.start, end: self.peek().span.start } });
        }
        let mut invariants = Vec::new();
        while matches!(self.peek().kind, TokenKind::Colon) {
            self.bump(); // :
            let kind = self.expect_ident("contract kind (:invariant)")?;
            if kind != "invariant" {
                return Err(self.err_here(
                    E2008,
                    format!("contract ':{}' is not allowed on a struct", kind),
                    "structs only accept :invariant; use :pre/:post on functions",
                ));
            }
            invariants.push(self.parse_expr()?);
        }
        let close = self.expect_rparen(&open)?;
        Ok(StructDef {
            node_id,
            name,
            fields,
            invariants,
            span: Span { start: open.span.start, end: close.span.end },
        })
    }

    fn parse_let(&mut self, open: Token) -> Result<LetDef, Diagnostic> {
        let node_id = self.alloc();
        let name = self.expect_ident("binding name")?;
        let ty = self.parse_type()?;
        let value = self.parse_expr()?;
        let close = self.expect_rparen(&open)?;
        Ok(LetDef {
            node_id,
            name,
            ty,
            value: Box::new(value),
            span: Span { start: open.span.start, end: close.span.end },
        })
    }

    fn parse_if(&mut self, open: Token) -> Result<Expr, Diagnostic> {
        let node_id = self.alloc();
        let cond = self.parse_expr_required("if", "3 arguments: (if cond then else)")?;
        let then_branch = self.parse_expr_required("if", "3 arguments: (if cond then else)")?;
        let else_branch = self.parse_expr_required("if", "3 arguments: (if cond then else)")?;
        let close = self.expect_rparen(&open)?;
        Ok(Expr {
            node_id,
            kind: ExprKind::If { cond: Box::new(cond), then_branch: Box::new(then_branch), else_branch: Box::new(else_branch) },
            span: Span { start: open.span.start, end: close.span.end },
        })
    }

    fn parse_block(&mut self, open: Token) -> Result<Expr, Diagnostic> {
        let node_id = self.alloc();
        let mut exprs = Vec::new();
        while !self.at_eof() && !matches!(self.peek().kind, TokenKind::RParen) {
            exprs.push(self.parse_expr()?);
        }
        let close = self.expect_rparen(&open)?;
        Ok(Expr {
            node_id,
            kind: ExprKind::Block(exprs),
            span: Span { start: open.span.start, end: close.span.end },
        })
    }

    fn parse_vec_lit(&mut self, open: Token) -> Result<Expr, Diagnostic> {
        let node_id = self.alloc();
        let mut items = Vec::new();
        while !self.at_eof() && !matches!(self.peek().kind, TokenKind::RParen) {
            items.push(self.parse_expr()?);
        }
        let close = self.expect_rparen(&open)?;
        Ok(Expr {
            node_id,
            kind: ExprKind::VecLit(items),
            span: Span { start: open.span.start, end: close.span.end },
        })
    }

    fn parse_map_lit(&mut self, open: Token) -> Result<Expr, Diagnostic> {
        let node_id = self.alloc();
        let mut pairs = Vec::new();
        while !self.at_eof() && !matches!(self.peek().kind, TokenKind::RParen) {
            let k_open = self.expect_lparen()?;
            let key = self.parse_expr_required("map-of", "pairs of the form (key value)")?;
            let value = self.parse_expr_required("map-of", "pairs of the form (key value)")?;
            self.expect_rparen(&k_open)?;
            pairs.push((key, value));
        }
        let close = self.expect_rparen(&open)?;
        Ok(Expr {
            node_id,
            kind: ExprKind::MapLit(pairs),
            span: Span { start: open.span.start, end: close.span.end },
        })
    }

    fn parse_quote(&mut self, open: Token) -> Result<Expr, Diagnostic> {
        let node_id = self.alloc();
        let inner = self.parse_expr_required("quote", "1 argument: (quote expr)")?;
        let close = self.expect_rparen(&open)?;
        Ok(Expr {
            node_id,
            kind: ExprKind::Quote(Box::new(inner)),
            span: Span { start: open.span.start, end: close.span.end },
        })
    }

    fn parse_call(&mut self, open: Token, name: String) -> Result<Expr, Diagnostic> {
        let node_id = self.alloc();
        let mut args = Vec::new();
        while !self.at_eof() && !matches!(self.peek().kind, TokenKind::RParen) {
            args.push(self.parse_expr()?);
        }
        let close = self.expect_rparen(&open)?;
        Ok(Expr {
            node_id,
            kind: ExprKind::Call { name, args },
            span: Span { start: open.span.start, end: close.span.end },
        })
    }

    // -- 表达式 --

    fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        match self.peek().kind.clone() {
            TokenKind::Int(i) => {
                let span = self.bump().span;
                Ok(Expr { node_id: self.alloc(), kind: ExprKind::Int(i), span })
            }
            TokenKind::Float(f) => {
                let span = self.bump().span;
                Ok(Expr { node_id: self.alloc(), kind: ExprKind::Float(f), span })
            }
            TokenKind::Str(s) => {
                let span = self.bump().span;
                Ok(Expr { node_id: self.alloc(), kind: ExprKind::Str(s), span })
            }
            TokenKind::Ident(name) => {
                let span = self.bump().span;
                let kind = match name.as_str() {
                    "true" => ExprKind::Bool(true),
                    "false" => ExprKind::Bool(false),
                    // 裸 none 等价于 (none):零元值构造器(与值规范表示 round-trip)
                    "none" => ExprKind::Call { name: "none".to_string(), args: vec![] },
                    _ => ExprKind::Var(name),
                };
                Ok(Expr { node_id: self.alloc(), kind, span })
            }
            TokenKind::LParen => match self.parse_form(false)? {
                Form::Expr(e) => Ok(e),
                Form::Fn(f) => {
                    let span = f.span;
                    Ok(Expr { node_id: f.node_id, kind: ExprKind::Fn(f), span })
                }
                _ => unreachable!("expr 上下文中 struct/module 已被 parse_form 拒绝"),
            },
            TokenKind::RParen => Err(self.err_here(
                E2003,
                "unexpected ')'".to_string(),
                "remove the extra ')' or check the bracket balance",
            )),
            TokenKind::Eof => Err(self.err_here(
                E2001,
                "unexpected end of input, expected an expression".to_string(),
                "check that every '(' has a matching ')'",
            )),
            _ => Err(self.err_here(
                E2001,
                "expected an expression".to_string(),
                "expressions are literals, identifiers, or parenthesized forms",
            )),
        }
    }

    fn parse_expr_required(&mut self, form: &str, arity_hint: &str) -> Result<Expr, Diagnostic> {
        if self.at_eof() || matches!(self.peek().kind, TokenKind::RParen) {
            return Err(self.err_here(
                E2004,
                format!("'{}' got too few arguments", form),
                arity_hint,
            ));
        }
        self.parse_expr()
    }

    // -- 类型 --

    fn parse_type(&mut self) -> Result<Ty, Diagnostic> {
        let start = self.peek().span;
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                self.bump();
                let kind = match name.as_str() {
                    "Unit" => TyKind::Unit,
                    "Int" => TyKind::Int,
                    "Float" => TyKind::Float,
                    "Bool" => TyKind::Bool,
                    "Str" => TyKind::Str,
                    "Ast" => TyKind::Ast,
                    _ => TyKind::Named(name),
                };
                Ok(Ty { kind, span: start })
            }
            TokenKind::LParen => {
                self.bump(); // (
                if matches!(self.peek().kind, TokenKind::RParen) {
                    let close = self.bump();
                    return Ok(Ty { kind: TyKind::Unit, span: Span { start: start.start, end: close.span.end } });
                }
                let head = match self.peek().kind.clone() {
                    TokenKind::Ident(n) => n,
                    _ => {
                        return Err(self.err_here(
                            E2007,
                            "expected a type constructor name".to_string(),
                            "compound types look like (Vec T), (Map K V), (Option T), (Result T E), (Fn (T*) -> T)",
                        ));
                    }
                };
                self.bump();
                let kind = match head.as_str() {
                    "Vec" => TyKind::VecOf(Box::new(self.parse_type()?)),
                    "Map" => TyKind::MapOf(Box::new(self.parse_type()?), Box::new(self.parse_type()?)),
                    "Option" => TyKind::Optional(Box::new(self.parse_type()?)),
                    "Result" => TyKind::ResultOf(Box::new(self.parse_type()?), Box::new(self.parse_type()?)),
                    "Fn" => {
                        // (Fn (T*) -> T),空参数列表写作 ()
                        let mut params = Vec::new();
                        let p_open = self.expect_lparen()?;
                        while !matches!(self.peek().kind, TokenKind::Arrow) {
                            if self.at_eof() || matches!(self.peek().kind, TokenKind::RParen) {
                                break;
                            }
                            params.push(self.parse_type()?);
                        }
                        self.expect_rparen(&p_open)?;
                        if !matches!(self.peek().kind, TokenKind::Arrow) {
                            return Err(self.err_here(
                                E2001,
                                "expected '->' in function type".to_string(),
                                "function types look like (Fn (ArgType*) -> RetType)",
                            ));
                        }
                        self.bump(); // ->
                        let ret = self.parse_type()?;
                        TyKind::Fn { params, ret: Box::new(ret) }
                    }
                    _ => {
                        return Err(self.err_here(
                            E2007,
                            format!("unknown type constructor '{}'", head),
                            "compound types look like (Vec T), (Map K V), (Option T), (Result T E), (Fn (T*) -> T)",
                        ));
                    }
                };
                let close = self.expect_rparen_typed()?;
                Ok(Ty { kind, span: Span { start: start.start, end: close.span.end } })
            }
            _ => Err(self.err_here(
                E2007,
                "expected a type".to_string(),
                "types are (), Unit, Int, Float, Bool, Str, Ast, (Vec T), (Map K V), (Option T), (Result T E), (Fn (T*) -> T) or a struct name",
            )),
        }
    }

    // -- 低层辅助 --

    fn expect_lparen(&mut self) -> Result<Token, Diagnostic> {
        if matches!(self.peek().kind, TokenKind::LParen) {
            Ok(self.bump())
        } else {
            Err(self.err_here(E2001, "expected '('".to_string(), "insert an opening '(' here"))
        }
    }

    fn expect_rparen(&mut self, open: &Token) -> Result<Token, Diagnostic> {
        if self.at_eof() {
            return Err(self.err(
                E2002,
                "unclosed '(', reached end of input".to_string(),
                Span { start: open.span.start, end: self.peek().span.start },
                "insert a closing ')' to match this opening bracket",
            ));
        }
        if matches!(self.peek().kind, TokenKind::RParen) {
            Ok(self.bump())
        } else {
            Err(self.err_here(
                E2001,
                "expected ')'".to_string(),
                "insert a closing ')' here",
            ))
        }
    }

    /// 类型解析的收尾:报错口径与 expect_rparen 一致,但面向 E2007 上下文。
    fn expect_rparen_typed(&mut self) -> Result<Token, Diagnostic> {
        if self.at_eof() {
            return Err(self.err_here(
                E2007,
                "unclosed type, reached end of input".to_string(),
                "insert a closing ')' to finish the type",
            ));
        }
        if matches!(self.peek().kind, TokenKind::RParen) {
            Ok(self.bump())
        } else {
            Err(self.err_here(
                E2007,
                "expected ')' to close the type".to_string(),
                "insert a closing ')' here",
            ))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, Diagnostic> {
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                self.bump();
                Ok(name)
            }
            _ => Err(self.err_here(
                E2006,
                format!("expected {} (an identifier)", what),
                &format!("insert an identifier here for the {}", what),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Program {
        match parse(src) {
            Ok(p) => p,
            Err(e) => panic!("expected parse success for {:?}, got {:?}", src, e),
        }
    }

    fn parse_err(src: &str) -> Diagnostic {
        match parse(src) {
            Err(e) => e,
            Ok(_) => panic!("expected parse error for {:?}", src),
        }
    }

    #[test]
    fn hello_program() {
        let p = parse_ok(r#"(module hello
  (fn main () -> () !
    (out "Hello, Aether!")))"#);
        assert_eq!(p.module.name.as_deref(), Some("hello"));
        assert_eq!(p.module.items.len(), 1);
        match &p.module.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.name.as_deref(), Some("main"));
                assert!(f.is_effectful);
                assert_eq!(f.params.len(), 0);
                assert_eq!(f.ret_ty.kind, TyKind::Unit);
                match &*f.body {
                    Expr { kind: ExprKind::Call { name, args }, .. } => {
                        assert_eq!(name, "out");
                        assert_eq!(args[0].kind, ExprKind::Str("Hello, Aether!".into()));
                    }
                    other => panic!("expected out call, got {:?}", other),
                }
            }
            other => panic!("expected fn item, got {:?}", other),
        }
    }

    #[test]
    fn anonymous_module() {
        let p = parse_ok("(fn add (a Int) (b Int) -> Int (+ a b))");
        assert_eq!(p.module.name, None);
        assert_eq!(p.module.items.len(), 1);
    }

    #[test]
    fn fib_with_contract() {
        let p = parse_ok(include_str!("../../../examples/fib.ae"));
        match &p.module.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.contracts.len(), 1);
                assert_eq!(f.contracts[0].kind, ContractKind::Pre);
            }
            other => panic!("expected fn, got {:?}", other),
        }
    }

    #[test]
    fn sort_with_anonymous_fn() {
        let p = parse_ok(include_str!("../../../examples/sort.ae"));
        match &p.module.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.contracts[0].kind, ContractKind::Post);
                match &*f.body {
                    Expr { kind: ExprKind::If { .. }, .. } => {}
                    other => panic!("expected if, got {:?}", other),
                }
            }
            other => panic!("expected fn, got {:?}", other),
        }
    }

    #[test]
    fn point_struct() {
        let p = parse_ok(include_str!("../../../examples/point.ae"));
        assert_eq!(p.module.items.len(), 2);
        match &p.module.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.name, "Point");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.invariants.len(), 1);
            }
            other => panic!("expected struct, got {:?}", other),
        }
    }

    #[test]
    fn quote_roundtrips_ast() {
        let p = parse_ok("(quote (+ 1 2))");
        match &p.module.items[0] {
            Item::Expr(Expr { kind: ExprKind::Quote(inner), .. }) => match &inner.kind {
                ExprKind::Call { name, args } => {
                    assert_eq!(name, "+");
                    assert_eq!(args.len(), 2);
                    assert_eq!(args[0].kind, ExprKind::Int(1));
                    assert_eq!(args[1].kind, ExprKind::Int(2));
                }
                other => panic!("expected call inside quote, got {:?}", other),
            },
            other => panic!("expected quote, got {:?}", other),
        }
    }

    #[test]
    fn node_ids_are_unique_and_allocated_in_order() {
        let p = parse_ok("(fn a (x Int) -> Int (+ x 1))");
        let mut ids = Vec::new();
        if let Item::Fn(f) = &p.module.items[0] {
            ids.push(f.node_id.0);
            if let Expr { node_id, kind: ExprKind::Call { name, args }, .. } = &*f.body {
                assert_eq!(name, "+");
                ids.push(node_id.0);
                for a in args {
                    ids.push(a.node_id.0);
                }
            }
        }
        // 分配顺序:fn(1) → call(2) → Var x(3) → Int 1(4)
        assert_eq!(ids, vec![1, 2, 3, 4]);
    }

    #[test]
    fn unclosed_paren_is_e2002() {
        let e = parse_err("(fn add (a Int) -> Int (+ a 1)");
        assert_eq!(e.code, E2002);
    }

    #[test]
    fn stray_rparen_is_e2003() {
        let e = parse_err(")");
        assert_eq!(e.code, E2003);
    }

    #[test]
    fn if_arity_is_e2004() {
        let e = parse_err("(if true 1)");
        assert_eq!(e.code, E2004);
    }

    #[test]
    fn nested_module_is_e2005() {
        let e = parse_err("(module a (module b 1))");
        assert_eq!(e.code, E2005);
    }

    #[test]
    fn struct_in_expr_position_is_e2005() {
        let e = parse_err("(fn f () -> () (struct S (x Int)))");
        assert_eq!(e.code, E2005);
    }

    #[test]
    fn atom_head_is_e2006() {
        let e = parse_err("(42 1)");
        assert_eq!(e.code, E2006);
    }

    #[test]
    fn missing_type_is_e2007() {
        let e = parse_err("(let x 1)");
        assert_eq!(e.code, E2007);
    }

    #[test]
    fn let_as_expression_in_block() {
        let p = parse_ok("(fn f () -> Int (block (let x Int 1) (+ x 1)))");
        match &p.module.items[0] {
            Item::Fn(f) => match &*f.body {
                Expr { kind: ExprKind::Block(exprs), .. } => {
                    assert_eq!(exprs.len(), 2);
                    match &exprs[0].kind {
                        ExprKind::Let(l) => assert_eq!(l.name, "x"),
                        other => panic!("expected let expr, got {:?}", other),
                    }
                }
                other => panic!("expected block, got {:?}", other),
            },
            other => panic!("expected fn, got {:?}", other),
        }
    }

    #[test]
    fn wrong_contract_on_fn_is_e2008() {
        let e = parse_err("(fn f () -> () :invariant true ())");
        assert_eq!(e.code, E2008);
    }

    #[test]
    fn wrong_contract_on_struct_is_e2008() {
        let e = parse_err("(struct S (x Int) :pre true)");
        assert_eq!(e.code, E2008);
    }

    #[test]
    fn fn_type_and_compound_types() {
        let p = parse_ok("(fn f (g (Fn (Int) -> Bool)) -> (Map Str (Vec Float)) (block))");
        match &p.module.items[0] {
            Item::Fn(f) => {
                match &f.params[0].ty.kind {
                    TyKind::Fn { params, ret } => {
                        assert_eq!(params.len(), 1);
                        assert_eq!(params[0].kind, TyKind::Int);
                        assert_eq!(ret.kind, TyKind::Bool);
                    }
                    other => panic!("expected Fn type, got {:?}", other),
                }
                match &f.ret_ty.kind {
                    TyKind::MapOf(k, v) => {
                        assert_eq!(k.kind, TyKind::Str);
                        match &v.kind {
                            TyKind::VecOf(inner) => assert_eq!(inner.kind, TyKind::Float),
                            other => panic!("expected VecOf, got {:?}", other),
                        }
                    }
                    other => panic!("expected MapOf, got {:?}", other),
                }
            }
            other => panic!("expected fn, got {:?}", other),
        }
    }
}
