//! Aether 抽象语法树(AST)定义。
//!
//! M0:仅占位。M1(WP1.2)将填充完整节点定义、`Span`、`NodeId` 与序列化。

/// 语言名与规范版本(供 CLI 与诊断使用)。
pub const LANGUAGE_NAME: &str = "aether";
pub const SPEC_VERSION: &str = "0.1.0-draft";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity() {
        assert_eq!(LANGUAGE_NAME, "aether");
        assert_eq!(SPEC_VERSION, "0.1.0-draft");
    }
}
