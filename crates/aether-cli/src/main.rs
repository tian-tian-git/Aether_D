//! Aether 命令行入口。
//!
//! M0:仅打印横幅。M1 起接入 `parse` 子命令(见 docs/roadmap.md WP1.6)。

use aether_ast::{LANGUAGE_NAME, SPEC_VERSION};

fn main() {
    println!("{} v{} (M0 scaffold — 路线图见 docs/roadmap.md)", LANGUAGE_NAME, SPEC_VERSION);
}
