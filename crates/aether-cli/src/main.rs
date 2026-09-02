//! Aether 命令行入口。
//!
//! 子命令:
//! - `aether parse <file> [--json]`  解析源码:成功打印 AST dump,失败输出诊断
//!   (`--json` 输出结构化诊断 JSON,供 M4 生成回路消费)。
//! - `aether run <file> [args...]`   运行程序,调用 `main`(退出码 = main 返回的 Int)。
//! - `aether repl`                   交互式求值(表达式一行,括号自动续行;`:q` 退出)。

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use aether_ast::{LANGUAGE_NAME, SPEC_VERSION};
use aether_interp::Interp;

fn usage() -> String {
    format!(
        "{} v{} — AI-native language (Cognitive Execution Protocol)\n\nUSAGE:\n  aether parse <file> [--json]   Parse a source file and dump its AST\n  aether run <file> [args...]    Run a program (exit code = main's Int return)\n  aether repl                    Interactive evaluation\n  aether --help                  Show this help\n",
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
        "run" => cmd_run(&args[2..]),
        "repl" => cmd_repl(),
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

fn cmd_run(args: &[String]) -> ExitCode {
    let Some(file) = args.first() else {
        eprintln!("error: 'aether run' requires a file argument\n\n{}", usage());
        return ExitCode::from(2);
    };
    let src = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", file, e);
            return ExitCode::from(2);
        }
    };
    let program = match aether_parser::parse(&src) {
        Ok(p) => p,
        Err(d) => {
            eprint!("{}", d.render(&src));
            return ExitCode::from(1);
        }
    };
    match Interp::new().run_program(&program, args[1..].to_vec()) {
        Ok(code) => ExitCode::from(code as u8),
        Err(d) => {
            eprint!("{}", d.render(&src));
            ExitCode::from(1)
        }
    }
}

fn cmd_repl() -> ExitCode {
    println!("aether repl v{} — type expressions; ':q' to quit", SPEC_VERSION);
    let stdin = io::stdin();
    let mut buffer = String::new();
    loop {
        let prompt = if buffer.trim().is_empty() { ">> " } else { ".. " };
        print!("{}", prompt);
        let _ = io::stdout().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("repl: read error: {}", e);
                return ExitCode::from(1);
            }
        }
        let line = line.trim_end().to_string();
        if buffer.trim().is_empty() && line == ":q" {
            break;
        }
        buffer.push_str(&line);
        buffer.push('\n');
        if !repl_complete(&buffer) {
            continue;
        }
        let src = buffer.clone();
        buffer.clear();
        match aether_parser::parse(&src) {
            Ok(program) => match Interp::new().eval_program(&program, vec![]) {
                Ok(aether_interp::Value::Unit) => {}
                Ok(v) => println!("{}", aether_interp::value::repr(&v)),
                Err(d) => eprintln!("{}", d.render(&src)),
            },
            Err(d) => eprintln!("{}", d.render(&src)),
        }
    }
    ExitCode::SUCCESS
}

/// 括号是否闭合(基于词法 token,忽略注释/字符串内括号)。
fn repl_complete(src: &str) -> bool {
    match aether_parser::lexer::lex(src) {
        Ok(tokens) => {
            let depth = tokens
                .iter()
                .fold(0i32, |d, t| match t.kind {
                    aether_parser::lexer::TokenKind::LParen => d + 1,
                    aether_parser::lexer::TokenKind::RParen => d - 1,
                    _ => d,
                });
            depth <= 0
        }
        Err(_) => true, // 词法错误交给 parse 报告
    }
}
