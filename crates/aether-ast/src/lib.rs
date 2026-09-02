//! Aether 抽象语法树(AST)定义。
//!
//! 规范:docs/spec/v0.1/01-syntax.md。
//!
//! ## API 契约
//! 本 crate 的 `Pos` / `Span` / `NodeId` 是跨 crate 公共接口,
//! 下游 `aether-parser` / `aether-diagnostic` / `aether-interp` 均按此编译,
//! 未经 ADR 不得更改其字段名与语义。
//! M1(WP1.2)将在此之上填充完整 AST 节点(由子智能体实现)。

/// 语言名与规范版本(供 CLI 与诊断使用)。
pub const LANGUAGE_NAME: &str = "aether";
pub const SPEC_VERSION: &str = "0.1.0-draft";

/// 源码位置:`offset` 为字节偏移,`line`/`col` 为 1 起算,`col` 按码点计数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pos {
    pub offset: usize,
    pub line: u32,
    pub col: u32,
}

impl Pos {
    /// 构造一个哑位置(用于无源码场景的测试与合成节点)。
    pub fn dummy() -> Self {
        Pos { offset: 0, line: 1, col: 1 }
    }
}

/// 源码区间,半开 `[start, end)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    /// 构造一个哑区间(用于无源码场景的测试与合成节点)。
    pub fn dummy() -> Self {
        Span { start: Pos::dummy(), end: Pos::dummy() }
    }

    /// 合并两个区间,取最早起点与最晚终点。
    pub fn merge(&self, other: &Span) -> Span {
        let start = if self.start.offset <= other.start.offset { self.start } else { other.start };
        let end = if self.end.offset >= other.end.offset { self.end } else { other.end };
        Span { start, end }
    }
}

/// AST 节点全局唯一编号;诊断与图补丁(远期)的锚点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity() {
        assert_eq!(LANGUAGE_NAME, "aether");
        assert_eq!(SPEC_VERSION, "0.1.0-draft");
    }

    #[test]
    fn span_merge_takes_earliest_and_latest() {
        let a = Span {
            start: Pos { offset: 10, line: 1, col: 11 },
            end: Pos { offset: 20, line: 1, col: 21 },
        };
        let b = Span {
            start: Pos { offset: 5, line: 1, col: 6 },
            end: Pos { offset: 30, line: 2, col: 3 },
        };
        let m = a.merge(&b);
        assert_eq!(m.start.offset, 5);
        assert_eq!(m.end.offset, 30);
    }

    #[test]
    fn dummy_spans_are_stable() {
        assert_eq!(Span::dummy(), Span::dummy());
        assert_eq!(Pos::dummy().line, 1);
    }

    #[test]
    fn node_id_ordering() {
        assert!(NodeId(1) < NodeId(2));
        assert_eq!(NodeId(7).0, 7);
    }
}
