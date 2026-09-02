//! Aether 结构化诊断(规范:docs/spec/v0.1/05-diagnostics.md)。
//!
//! 诊断是「AI 生成回路」(M4)的地基:每条诊断均可被程序消费(JSON)、
//! 可被 LLM 直接利用(稳定错误码 + 修复建议)。JSON 为手工序列化,
//! 不依赖外部 crate,格式与字段顺序以 05-diagnostics.md 为准。

use aether_ast::{NodeId, Span};

/// 诊断严重级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Hint => write!(f, "hint"),
        }
    }
}

/// 一条结构化诊断。
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    /// AST 节点编号;词法阶段错误无节点,缺省。
    pub node_id: Option<NodeId>,
    pub severity: Severity,
    /// 稳定错误码(如 "E2001"),是程序消费的第一键。
    pub code: String,
    /// 英文消息,面向 LLM 语料。
    pub message: String,
    pub span: Span,
    /// 面向「下一轮生成」的修复建议,至少一条。
    pub hints: Vec<String>,
}

impl Diagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Error, code, message, span)
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Warning, code, message, span)
    }

    pub fn hint(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Hint, code, message, span)
    }

    fn new(severity: Severity, code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Diagnostic { node_id: None, severity, code: code.into(), message: message.into(), span, hints: Vec::new() }
    }

    /// 链式:设置节点编号。
    pub fn with_node_id(mut self, id: NodeId) -> Self {
        self.node_id = Some(id);
        self
    }

    /// 链式:追加一条修复建议。
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }

    /// 单条 JSON(05-diagnostics.md §2 Schema,字段顺序固定,紧凑单行)。
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        if let Some(id) = self.node_id {
            s.push_str(&format!("\"node_id\":{},", id.0));
        }
        s.push_str(&format!(
            "\"severity\":\"{}\",\"code\":\"{}\",\"message\":\"{}\",",
            self.severity,
            escape(&self.code),
            escape(&self.message)
        ));
        let (a, b) = (self.span.start, self.span.end);
        s.push_str(&format!(
            "\"span\":{{\"start\":{{\"offset\":{},\"line\":{},\"col\":{}}},\"end\":{{\"offset\":{},\"line\":{},\"col\":{}}}}},\"hints\":[",
            a.offset, a.line, a.col, b.offset, b.line, b.col
        ));
        for (i, h) in self.hints.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("\"{}\"", escape(h)));
        }
        s.push_str("]}");
        s
    }

    /// 人类可读渲染(05-diagnostics.md §5)。
    pub fn render(&self, source: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("{}[{}]: {}\n", self.severity, self.code, self.message));
        let start = self.span.start;
        out.push_str(&format!("  --> {}:{}\n", start.line, start.col));
        let lines: Vec<&str> = source.lines().collect();
        if let Some(line) = lines.get((start.line as usize).saturating_sub(1)) {
            out.push_str(&format!("{:>4} | {}\n", start.line, line));
            let col = start.col as usize;
            let end_col = if self.span.end.line == start.line {
                self.span.end.col as usize
            } else {
                line.chars().count() + 1
            };
            let len = end_col.saturating_sub(col).max(1);
            out.push_str(&format!("     | {}{}\n", " ".repeat(col.saturating_sub(1)), "^".repeat(len)));
        }
        for h in &self.hints {
            out.push_str(&format!("   = hint: {}\n", h));
        }
        out
    }
}

/// JSON 字符串转义:仅 `"` `\` `\n` `\t` `\r` 具名,其余控制字符 \uXXXX。
fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// 批量 JSON:`{"diagnostics":[...]}`(05-diagnostics.md §2)。
pub fn to_json_batch(diagnostics: &[Diagnostic]) -> String {
    let mut s = String::from("{\"diagnostics\":[");
    for (i, d) in diagnostics.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&d.to_json());
    }
    s.push_str("]}");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ast::Pos;

    fn span(a: (usize, u32, u32), b: (usize, u32, u32)) -> Span {
        Span {
            start: Pos { offset: a.0, line: a.1, col: a.2 },
            end: Pos { offset: b.0, line: b.1, col: b.2 },
        }
    }

    #[test]
    fn golden_json_without_node_id() {
        let d = Diagnostic::error("E2001", "expected ')'", span((120, 6, 1), (121, 6, 2)))
            .with_hint("insert a closing ')' here");
        let expected = concat!(
            r#"{"severity":"error","code":"E2001","message":"expected ')'","#,
            r#""span":{"start":{"offset":120,"line":6,"col":1},"end":{"offset":121,"line":6,"col":2}},"#,
            r#""hints":["insert a closing ')' here"]}"#
        );
        assert_eq!(d.to_json(), expected);
    }

    #[test]
    fn json_with_node_id_places_it_first() {
        let d = Diagnostic::error("E3001", "type mismatch", span((0, 1, 1), (1, 1, 2))).with_node_id(NodeId(42));
        let json = d.to_json();
        assert!(json.starts_with(r#"{"node_id":42,"severity":"error""#));
    }

    #[test]
    fn json_escaping() {
        let d = Diagnostic::error(
            "E2001",
            "bad \"quote\" \\ and\nnewline\tand \u{1}",
            span((0, 1, 1), (1, 1, 2)),
        );
        let json = d.to_json();
        assert!(json.contains(r#"bad \"quote\" \\ and\nnewline\tand \u0001"#));
    }

    #[test]
    fn json_batch() {
        assert_eq!(to_json_batch(&[]), r#"{"diagnostics":[]}"#);
        let a = Diagnostic::error("E1001", "a", span((0, 1, 1), (1, 1, 2)));
        let b = Diagnostic::warning("E1002", "b", span((2, 1, 3), (3, 1, 4)));
        let json = to_json_batch(&[a, b]);
        assert!(json.starts_with(r#"{"diagnostics":[{"severity":"error""#));
        assert!(json.contains(r#"},{"severity":"warning""#));
        assert!(json.ends_with("]}"));
    }

    #[test]
    fn render_shows_line_and_caret() {
        let d = Diagnostic::error("E2001", "expected ')'", span((7, 2, 3), (8, 2, 4)));
        let rendered = d.render("(let x Int 1\n(let y Int 2)\n");
        assert!(rendered.contains("error[E2001]: expected ')'"));
        assert!(rendered.contains("--> 2:3"));
        assert!(rendered.contains("2 | (let y Int 2)"));
        assert!(rendered.contains("     |   ^"));
    }

    #[test]
    fn render_without_source_omits_code_lines() {
        let d = Diagnostic::error("E1003", "illegal character '#'", span((0, 1, 1), (1, 1, 2)))
            .with_hint("remove the character");
        let rendered = d.render("");
        assert!(rendered.contains("--> 1:1"));
        assert!(!rendered.contains(" | "));
        assert!(rendered.contains("= hint: remove the character"));
    }

    #[test]
    fn builder_chain() {
        let d = Diagnostic::error("E1", "m", span((0, 1, 1), (1, 1, 2)))
            .with_node_id(NodeId(7))
            .with_hint("h1")
            .with_hint("h2");
        assert_eq!(d.node_id, Some(NodeId(7)));
        assert_eq!(d.hints, vec!["h1".to_string(), "h2".to_string()]);
        assert_eq!(d.severity, Severity::Error);
    }

    #[test]
    fn severity_display() {
        assert_eq!(Severity::Error.to_string(), "error");
        assert_eq!(Severity::Warning.to_string(), "warning");
        assert_eq!(Severity::Hint.to_string(), "hint");
    }
}
