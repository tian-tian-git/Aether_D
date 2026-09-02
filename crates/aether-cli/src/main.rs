//! Aether 命令行入口。
//!
//! 子命令(M1):
//! - `aether parse <file> [--json]`  解析源码:成功打印 AST dump,失败输出诊断
//!   (`--json` 输出结构化诊断 JSON,供 M4 生成回路消费)。

use std::env;
use std::fs;
use std::process::ExitCode;

use aether_ast::{LANGUAGE_NAME, SPEC_VERSION};

fn usage() -> String {
    format!(
        "{} v{} — AI-native language (Cognitive Execution Protocol)\n\nUSAGE:\n  aether parse <file> [--json]   Parse a source file and dump its AST\n  aether --help                Show this help\n",
        LANGUAGE_NAME, SPEC_VERSION
    )
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print!("{}", usage());
        return ExitCode::from(if args.len() < 2 { 2 } else { 0 });
    }
    match args[1].as_str() {
        "parse" => cmd_parse(&args[2..]),
        other => {
            eprintln!("unknown subcommand '{}'\n\n{}", other, usage());
            ExitCode::from(2)
        }
    }
}

fn cmd_parse(args: &[String]) -> ExitCode {
    let json = args.iter().any(|a| a == "--json");
    let files: Vec<&String> = args.iter().filter(|a| *a != "--json").collect();
    let Some(file) = files.first() else {
        eprintln!("error: 'aether parse' requires a file argument\n\n{}", usage());
        return ExitCode::from(2);
    };
    let src = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", file, e);
            return ExitCode::from(2);
        }
    };
    match aether_parser::parse(&src) {
        Ok(program) => {
            if json {
                println!("{}", "{\"ok\":true}");
            } else {
                println!("{:#?}", program);
            }
            ExitCode::SUCCESS
        }
        Err(d) => {
            if json {
                println!("{}", d.to_json());
            } else {
                eprint!("{}", d.render(&src));
            }
            ExitCode::from(1)
        }
    }
}
