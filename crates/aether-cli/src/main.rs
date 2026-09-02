//! Aether 命令行入口。
//!
//! 子命令:
//! - `aether parse <file> [--json]`  解析源码:成功打印 AST dump,失败输出诊断
//!   (`--json` 输出结构化诊断 JSON,供 M4 生成回路消费)。
//! - `aether check <file> [--json]`  静态类型检查(E3xxx 诊断)。
//! - `aether run <file> [args...]`   类型检查通过后运行,调用 `main`(退出码 = main 返回的 Int)。
//! - `aether repl`                   交互式求值(表达式一行,括号自动续行;`:q` 退出)。

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use aether_ast::{LANGUAGE_NAME, SPEC_VERSION};
use aether_interp::Interp;

fn usage() -> String {
    format!(
        "{} v{} — AI-native language (Cognitive Execution Protocol)\n\nUSAGE:\n  aether parse <file> [--json]   Parse a source file and dump its AST\n  aether check <file> [--json]   Static type check (E3xxx diagnostics)\n  aether run <file> [args...]    Type check, then run (exit code = main's Int return)\n  aether repl                    Interactive evaluation\n  aether --help                  Show this help\n",
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
        "check" => cmd_check(&args[2..]),
        "run" => cmd_run(&args[2..]),
        "repl" => cmd_repl(),
        other => {
            eprintln!("unknown subcommand '{}'\n\n{}", other, usage());
            ExitCode::from(2)
        }
    }
}

fn read_source(file: &str) -> Result<String, ExitCode> {
    fs::read_to_string(file).map_err(|e| {
        eprintln!("error: cannot read '{}': {}", file, e);
        ExitCode::from(2)
    })
}

fn parse_or_exit(src: &str) -> Result<aether_ast::Program, ExitCode> {
    aether_parser::parse(src).map_err(|d| {
        eprint!("{}", d.render(src));
        ExitCode::from(1)
    })
}

fn cmd_parse(args: &[String]) -> ExitCode {
    let json = args.iter().any(|a| a == "--json");
    let files: Vec<&String> = args.iter().filter(|a| *a != "--json").collect();
    let Some(file) = files.first() else {
        eprintln!("error: 'aether parse' requires a file argument\n\n{}", usage());
        return ExitCode::from(2);
    };
    let src = match read_source(file) {
        Ok(s) => s,
        Err(c) => return c,
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

fn cmd_check(args: &[String]) -> ExitCode {
    let json = args.iter().any(|a| a == "--json");
    let files: Vec<&String> = args.iter().filter(|a| *a != "--json").collect();
    let Some(file) = files.first() else {
        eprintln!("error: 'aether check' requires a file argument\n\n{}", usage());
        return ExitCode::from(2);
    };
    let src = match read_source(file) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let program = match parse_or_exit(&src) {
        Ok(p) => p,
        Err(c) => return c,
    };
    match aether_verify::check_program(&program) {
        Ok(()) => {
            // M3 起:类型检查后执行静态契约验证(Z3)
            let verify_result = match aether_verify::try_load() {
                Ok(api) => {
                    let diags = aether_verify::verify_program(&api, &program);
                    if diags.is_empty() {
                        Ok(())
                    } else {
                        Err(diags)
                    }
                }
                Err(reason) => {
                    if json {
                        println!("{}", format!("{{\"ok\":true,\"verify\":\"skipped\",\"reason\":\"{}\"}}", reason.replace('"', "'")));
                        return ExitCode::SUCCESS;
                    }
                    println!("ok: type check passed (static contract verification skipped: {})", reason);
                    return ExitCode::SUCCESS;
                }
            };
            match verify_result {
                Ok(()) => {
                    if json {
                        println!("{}", "{\"ok\":true,\"verify\":\"verified\"}");
                    } else {
                        println!("ok: type check passed; static contract verification: no provable violations");
                    }
                    ExitCode::SUCCESS
                }
                Err(diags) => {
                    if json {
                        println!("{}", aether_diagnostic::to_json_batch(&diags));
                    } else {
                        for d in &diags {
                            eprint!("{}", d.render(&src));
                        }
                    }
                    ExitCode::from(1)
                }
            }
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
    // M5 起默认字节码 VM;--interp 回退树遍历解释器
    let use_interp = args.iter().any(|a| a == "--interp");
    let files: Vec<&String> = args.iter().filter(|a| *a != "--interp" && *a != "--vm").collect();
    let Some(file) = files.first() else {
        eprintln!("error: 'aether run' requires a file argument\n\n{}", usage());
        return ExitCode::from(2);
    };
    let argv: Vec<String> = files.iter().skip(1).map(|s| s.to_string()).collect();
    let src = match read_source(file) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let program = match parse_or_exit(&src) {
        Ok(p) => p,
        Err(c) => return c,
    };
    // M3 起:运行前静态类型检查
    if let Err(d) = aether_verify::check_program(&program) {
        eprint!("{}", d.render(&src));
        return ExitCode::from(1);
    }
    let result = if use_interp {
        Interp::new().run_program(&program, argv)
    } else {
        aether_vm::run(&program, argv)
    };
    match result {
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
            Ok(program) => {
                // M3 起:REPL 求值前静态类型检查
                if let Err(d) = aether_verify::check_program(&program) {
                    eprintln!("{}", d.render(&src));
                    continue;
                }
                match Interp::new().eval_program(&program, vec![]) {
                    Ok(aether_interp::Value::Unit) => {}
                    Ok(v) => println!("{}", aether_interp::value::repr(&v)),
                    Err(d) => eprintln!("{}", d.render(&src)),
                }
            }
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
