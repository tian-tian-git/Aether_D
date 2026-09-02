//! 词法分析器(规范:docs/spec/v0.1/01-syntax.md §2)。
//!
//! 歧义消解固定规则(与规范一致):
//! 1. `-` 后紧跟数字 → 负数字面量;`0x` 后跟十六进制位 → 十六进制整数。
//! 2. 数字字面量优先于标识符。
//! 3. `->` 双字符 token 优先于两个单字符。
//! 4. `;` 必须成对出现(`;;` 注释),单独出现报 E1001。
//! 5. `!` 单独出现为效果标记 token;`!=` 起首则为标识符(`!=` 是比较内建)。
//!
//! 错误码(注册于 docs/spec/v0.1/05-diagnostics.md):
//! E1001 孤立分号 / E1002 未闭合字符串 / E1003 非法字符 /
//! E1004 整数字面量溢出 / E1005 非法转义序列 / E1006 非法数字字面量。

use aether_ast::{Pos, Span};

pub const E1001: &str = "E1001";
pub const E1002: &str = "E1002";
pub const E1003: &str = "E1003";
pub const E1004: &str = "E1004";
pub const E1005: &str = "E1005";
pub const E1006: &str = "E1006";

/// 词法错误:结构化(错误码 + 消息 + 区间 + 修复建议)。
/// WP1.3 起由解析层转换为 `aether-diagnostic::Diagnostic`。
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    LParen,
    RParen,
    Colon,
    Arrow,
    Bang,
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// 将源码切分为 token 流;失败返回首个词法错误。
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(src).run()
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '_' | '+' | '-' | '*' | '/' | '%' | '<' | '>' | '=' | '!' | '?' | '.')
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

struct Lexer<'a> {
    src: &'a str,
    chars: Vec<char>,
    /// 当前字符在 chars 中的下标。
    i: usize,
    /// chars[i] 对应的字节偏移。
    offset: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer { src, chars: src.chars().collect(), i: 0, offset: 0, line: 1, col: 1 }
    }

    fn pos(&self) -> Pos {
        Pos { offset: self.offset, line: self.line, col: self.col }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.i + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.i).copied()?;
        self.i += 1;
        self.offset += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn run(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_trivia()?;
            let start = self.pos();
            let Some(c) = self.peek() else {
                tokens.push(Token { kind: TokenKind::Eof, span: Span { start, end: start } });
                return Ok(tokens);
            };
            let kind = match c {
                '(' => { self.advance(); TokenKind::LParen }
                ')' => { self.advance(); TokenKind::RParen }
                ':' => { self.advance(); TokenKind::Colon }
                '!' => {
                    if self.peek2() == Some('=') {
                        self.lex_ident(start)?
                    } else {
                        self.advance();
                        TokenKind::Bang
                    }
                }
                '-' => {
                    if self.peek2() == Some('>') {
                        self.advance();
                        self.advance();
                        TokenKind::Arrow
                    } else if self.peek2().is_some_and(|d| d.is_ascii_digit()) {
                        self.lex_number(start)?
                    } else {
                        self.lex_ident(start)?
                    }
                }
                '0'..='9' => self.lex_number(start)?,
                '"' => self.lex_string(start)?,
                c if is_ident_start(c) => self.lex_ident(start)?,
                _ => {
                    self.advance();
                    return Err(LexError {
                        code: E1003,
                        message: format!("illegal character '{}'", c.escape_default()),
                        span: Span { start, end: self.pos() },
                        hint: "remove the character or replace it with a valid token".to_string(),
                    });
                }
            };
            tokens.push(Token { kind, span: Span { start, end: self.pos() } });
        }
    }

    fn skip_trivia(&mut self) -> Result<(), LexError> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => { self.advance(); }
                Some(';') => {
                    let start = self.pos();
                    self.advance();
                    if self.peek() == Some(';') {
                        // 行注释:吞到行尾(不含换行)。
                        while let Some(c) = self.peek() {
                            if c == '\n' { break; }
                            self.advance();
                        }
                    } else {
                        return Err(LexError {
                            code: E1001,
                            message: "stray ';'".to_string(),
                            span: Span { start, end: self.pos() },
                            hint: "use ';;' to start a line comment".to_string(),
                        });
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn lex_ident(&mut self, _start: Pos) -> Result<TokenKind, LexError> {
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                name.push(c);
                self.advance();
            } else {
                break;
            }
        }
        Ok(TokenKind::Ident(name))
    }

    fn lex_number(&mut self, start: Pos) -> Result<TokenKind, LexError> {
        let start_offset = self.offset;
        let mut is_float = false;

        if self.peek() == Some('-') {
            self.advance();
        }
        // 十六进制整数
        if self.peek() == Some('0') && matches!(self.peek2(), Some('x') | Some('X')) {
            self.advance();
            self.advance();
            let mut any = false;
            while self.peek().is_some_and(is_hex) {
                self.advance();
                any = true;
            }
            if !any {
                return Err(LexError {
                    code: E1006,
                    message: "hex literal requires at least one hex digit".to_string(),
                    span: Span { start, end: self.pos() },
                    hint: "write e.g. 0xFF".to_string(),
                });
            }
            let raw = &self.src[start_offset..self.offset];
            let negative = raw.starts_with('-');
            let body = if negative { &raw[1..] } else { raw };
            let digits = body
                .strip_prefix("0x")
                .or_else(|| body.strip_prefix("0X"))
                .unwrap_or(body);
            let value = i128::from_str_radix(digits, 16).map_err(|_| LexError {
                code: E1006,
                message: "invalid hex literal".to_string(),
                span: Span { start, end: self.pos() },
                hint: "write e.g. 0xFF".to_string(),
            })?;
            let value = if negative { -value } else { value };
            if !(i64::MIN as i128..=i64::MAX as i128).contains(&value) {
                return Err(LexError {
                    code: E1004,
                    message: "integer literal overflows 64-bit signed range".to_string(),
                    span: Span { start, end: self.pos() },
                    hint: "keep integer literals within i64 range".to_string(),
                });
            }
            return Ok(TokenKind::Int(value as i64));
        }

        // 十进制整数 / 浮点
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.advance();
        }
        if self.peek() == Some('.') {
            is_float = true;
            self.advance();
            let mut any = false;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
                any = true;
            }
            if !any {
                return Err(LexError {
                    code: E1006,
                    message: "float literal requires digits after the dot".to_string(),
                    span: Span { start, end: self.pos() },
                    hint: "write e.g. 3.0 or 0.5 (both sides of the dot need digits)".to_string(),
                });
            }
            if matches!(self.peek(), Some('e') | Some('E')) {
                self.advance();
                if matches!(self.peek(), Some('+') | Some('-')) {
                    self.advance();
                }
                let mut any = false;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                    any = true;
                }
                if !any {
                    return Err(LexError {
                        code: E1006,
                        message: "float exponent requires digits".to_string(),
                        span: Span { start, end: self.pos() },
                        hint: "write e.g. 1.0e3 or 2.5e-2".to_string(),
                    });
                }
            }
        }
        // 数字后紧跟标识符字符 → 非法(如 1abc / 1-2 / 1.5x)
        if self.peek().is_some_and(is_ident_start) {
            return Err(LexError {
                code: E1006,
                message: "number literal is immediately followed by an identifier character".to_string(),
                span: Span { start, end: self.pos() },
                hint: "separate the number and the identifier with whitespace or parentheses".to_string(),
            });
        }
        let raw = &self.src[start_offset..self.offset];
        if is_float {
            let value: f64 = raw.parse().map_err(|_| LexError {
                code: E1006,
                message: format!("invalid float literal '{}'", raw),
                span: Span { start, end: self.pos() },
                hint: "write e.g. 3.14 or 1.0e3".to_string(),
            })?;
            if !value.is_finite() {
                return Err(LexError {
                    code: E1004,
                    message: "float literal overflows the finite f64 range".to_string(),
                    span: Span { start, end: self.pos() },
                    hint: "use a smaller magnitude; infinities and NaN are not literals in v0.1".to_string(),
                });
            }
            Ok(TokenKind::Float(value))
        } else {
            let value: i64 = raw.parse().map_err(|_| LexError {
                code: E1004,
                message: "integer literal overflows 64-bit signed range".to_string(),
                span: Span { start, end: self.pos() },
                hint: "keep integer literals within i64 range".to_string(),
            })?;
            Ok(TokenKind::Int(value))
        }
    }

    fn lex_string(&mut self, start: Pos) -> Result<TokenKind, LexError> {
        self.advance(); // 吃掉开引号
        let mut value = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => {
                    return Err(LexError {
                        code: E1002,
                        message: "unterminated string literal".to_string(),
                        span: Span { start, end: self.pos() },
                        hint: "close the string with a double quote".to_string(),
                    });
                }
                Some('"') => {
                    self.advance();
                    return Ok(TokenKind::Str(value));
                }
                Some('\\') => {
                    let esc_start = self.pos();
                    self.advance();
                    match self.peek() {
                        Some('n') => { value.push('\n'); self.advance(); }
                        Some('t') => { value.push('\t'); self.advance(); }
                        Some('r') => { value.push('\r'); self.advance(); }
                        Some('\\') => { value.push('\\'); self.advance(); }
                        Some('"') => { value.push('"'); self.advance(); }
                        Some('u') => {
                            self.advance();
                            if self.peek() != Some('{') {
                                return Err(LexError {
                                    code: E1005,
                                    message: "unicode escape must look like \\u{XXXX}".to_string(),
                                    span: Span { start: esc_start, end: self.pos() },
                                    hint: "write \\u{...} with 1-6 hex digits".to_string(),
                                });
                            }
                            self.advance();
                            let mut hex = String::new();
                            while self.peek().is_some_and(is_hex) && hex.len() < 6 {
                                hex.push(self.advance().unwrap());
                            }
                            if hex.is_empty() || self.peek() != Some('}') {
                                return Err(LexError {
                                    code: E1005,
                                    message: "unicode escape must look like \\u{XXXX}".to_string(),
                                    span: Span { start: esc_start, end: self.pos() },
                                    hint: "write \\u{...} with 1-6 hex digits".to_string(),
                                });
                            }
                            self.advance(); // }
                            let cp = u32::from_str_radix(&hex, 16).unwrap_or(u32::MAX);
                            match char::from_u32(cp) {
                                Some(ch) => value.push(ch),
                                None => {
                                    return Err(LexError {
                                        code: E1005,
                                        message: format!("invalid unicode scalar value U+{:X}", cp),
                                        span: Span { start: esc_start, end: self.pos() },
                                        hint: "use a valid unicode scalar value (not a surrogate, <= U+10FFFF)".to_string(),
                                    });
                                }
                            }
                        }
                        _ => {
                            let bad = self.advance();
                            return Err(LexError {
                                code: E1005,
                                message: format!("invalid escape sequence '\\{}'", bad.map(|c| c.to_string()).unwrap_or_default()),
                                span: Span { start: esc_start, end: self.pos() },
                                hint: "use one of \\n \\t \\r \\\\ \\\" \\u{XXXX}".to_string(),
                            });
                        }
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.advance();
                }
            }
        }
    }
}

fn is_hex(c: char) -> bool {
    c.is_ascii_hexdigit()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    fn err(src: &str) -> LexError {
        match lex(src) {
            Err(e) => e,
            Ok(_) => panic!("expected lex error for {:?}", src),
        }
    }

    #[test]
    fn hello_program_tokens() {
        let ks = kinds(r#"(module hello
  (fn main () -> () !
    (out "Hello, Aether!")))"#);
        assert_eq!(
            ks,
            vec![
                TokenKind::LParen,
                TokenKind::Ident("module".into()),
                TokenKind::Ident("hello".into()),
                TokenKind::LParen,
                TokenKind::Ident("fn".into()),
                TokenKind::Ident("main".into()),
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Arrow,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Bang,
                TokenKind::LParen,
                TokenKind::Ident("out".into()),
                TokenKind::Str("Hello, Aether!".into()),
                TokenKind::RParen,
                TokenKind::RParen,
                TokenKind::RParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(kinds("42"), vec![TokenKind::Int(42), TokenKind::Eof]);
        assert_eq!(kinds("-7"), vec![TokenKind::Int(-7), TokenKind::Eof]);
        assert_eq!(kinds("0xFF"), vec![TokenKind::Int(255), TokenKind::Eof]);
        assert_eq!(kinds("-0x10"), vec![TokenKind::Int(-16), TokenKind::Eof]);
        assert_eq!(kinds("3.14"), vec![TokenKind::Float(3.14), TokenKind::Eof]);
        assert_eq!(kinds("-0.5"), vec![TokenKind::Float(-0.5), TokenKind::Eof]);
        assert_eq!(kinds("1.0e3"), vec![TokenKind::Float(1000.0), TokenKind::Eof]);
        assert_eq!(kinds("2.5e-2"), vec![TokenKind::Float(0.025), TokenKind::Eof]);
    }

    #[test]
    fn minus_alone_is_ident() {
        assert_eq!(kinds("(- 1 2)"), vec![
            TokenKind::LParen,
            TokenKind::Ident("-".into()),
            TokenKind::Int(1),
            TokenKind::Int(2),
            TokenKind::RParen,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(
            kinds(r#""a\nb\t\u{4F60}\"\\""#),
            vec![TokenKind::Str("a\nb\t你\"\\".into()), TokenKind::Eof]
        );
    }

    #[test]
    fn idents_kebab_operator_predicate() {
        assert_eq!(
            kinds("get-user-name <= sorted? . !="),
            vec![
                TokenKind::Ident("get-user-name".into()),
                TokenKind::Ident("<=".into()),
                TokenKind::Ident("sorted?".into()),
                TokenKind::Ident(".".into()),
                TokenKind::Ident("!=".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn bang_vs_neq() {
        assert_eq!(kinds("!"), vec![TokenKind::Bang, TokenKind::Eof]);
        assert_eq!(kinds("!="), vec![TokenKind::Ident("!=".into()), TokenKind::Eof]);
    }

    #[test]
    fn comments_and_trivia() {
        assert_eq!(kinds(";; hi there\n42 ;; tail"), vec![TokenKind::Int(42), TokenKind::Eof]);
    }

    #[test]
    fn stray_semicolon_is_e1001() {
        let e = err(";");
        assert_eq!(e.code, E1001);
        assert_eq!(e.span.start.offset, 0);
    }

    #[test]
    fn unterminated_string_is_e1002() {
        assert_eq!(err("\"abc").code, E1002);
        assert_eq!(err("\"ab\nc\"").code, E1002);
    }

    #[test]
    fn illegal_char_is_e1003() {
        let e = err("(#)");
        assert_eq!(e.code, E1003);
        assert_eq!(e.span.start.offset, 1);
    }

    #[test]
    fn int_overflow_is_e1004() {
        assert_eq!(err("9223372036854775808").code, E1004);
        assert_eq!(err("0x8000000000000000").code, E1004);
    }

    #[test]
    fn non_finite_float_is_e1004() {
        assert_eq!(err("1.0e999").code, E1004);
    }

    #[test]
    fn bad_escape_is_e1005() {
        assert_eq!(err(r#""\q""#).code, E1005);
        assert_eq!(err(r#""\u{}""#).code, E1005);
        assert_eq!(err(r#""\u{D800}""#).code, E1005);
    }

    #[test]
    fn bad_numbers_are_e1006() {
        assert_eq!(err("1.5x").code, E1006);
        assert_eq!(err("1-2").code, E1006);
        assert_eq!(err("1abc").code, E1006);
        assert_eq!(err("1.").code, E1006);
        assert_eq!(err("0x").code, E1006);
        assert_eq!(err("1e3").code, E1006);
    }

    #[test]
    fn spans_track_lines_and_cols() {
        let ts = lex("(a\n  bb)").unwrap();
        let bb = &ts[2];
        assert_eq!(bb.span.start.line, 2);
        assert_eq!(bb.span.start.col, 3);
        let a = &ts[1];
        assert_eq!(a.span.start.line, 1);
        assert_eq!(a.span.start.col, 2);
    }

    #[test]
    fn byte_offsets_respect_utf8() {
        // '你' 占 3 字节;col 按码点、offset 按字节。
        let ts = lex("\"你\"").unwrap();
        let s = &ts[0];
        assert_eq!(s.span.end.offset, 5); // " 你 "
        assert_eq!(s.span.end.col, 4);
    }
}
