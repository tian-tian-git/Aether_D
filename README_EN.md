# Aether — An AI-Native Programming Language (Cognitive Execution Protocol)

> A programming language built **for AI, not for humans**: AI generates the code, the language verifies it, and it can bootstrap itself.

[English](README_EN.md) | **[中文](README.md)**

## One-Line Positioning

Today, AI writes code in a roundabout way: **AI → human syntax (Python/C++) → compiler → machine code**, with a layer of human-designed language in between — full of ambiguity, implicit rules, and legacy baggage. **Aether removes that middle layer**: AI emits a logic topology (a structured AST) directly, and the compiler handles verification (contracts / SMT) and lowering (interpretation / compilation) — pushing LLMs from "probabilistic code copiers" toward "deterministic logic provers".

We **don't reinvent wheels**: standing on LLVM / MLIR / Z3 and the lessons of existing AI-language projects (KARN, Zero, IF-Lang, …), we first build "an ultra-thin semantic glue layer between AI and existing hardware", and evolve from there.

## Core Design Pillars

1. **Unambiguous syntax (AST *is* the syntax)**: S-expression style; no operator precedence, no implicit conversions; parse tree == semantic tree; minimal token cost.
2. **Contract-driven (Design by Contract)**: `pre` / `post` / `invariant` are first-class syntax; runtime checks as the safety net, with Z3 for static verification over numeric domains.
3. **Immutable by default + explicit effects**: value semantics, pure functions by default, side effects must be declared; the goal is zero undefined behavior.
4. **Homoiconicity**: code is data — reflection and structural patching; AI edits *nodes*, not text.
5. **AI-first tooling**: structured diagnostics (node ID + contract ID + fix hints), a generate–verify–repair loop, and token metrics as first-class citizens.

## Pragmatic Boundaries (what we deliberately don't do)

- We don't replace von Neumann hardware, and we don't compete head-on with Python / C++: Aether is "the semantic glue layer between AI and existing hardware".
- We don't optimize for human readability; humans review code through a "de-renderer" (future work).
- We don't jump straight to MLIR / LLVM: first lock down semantics with a tree-walking interpreter, then add performance backends incrementally.

## Repository Layout

```
Aether_D/
├── README.md / README_EN.md   # Project overview (中文 / English)
├── AGENTS.md                  # Operating manual for AI agents (required reading)
├── AI跨语言设计.txt            # Original design conversations (read-only archive)
├── docs/
│   ├── vision.md              # Vision & design pillars
│   ├── roadmap.md             # Milestone roadmap (exit-criteria driven, no calendar time)
│   ├── architecture.md        # Four-layer architecture & compile pipeline
│   ├── spec/v0.1/             # Language spec (syntax/semantics/types/contracts/diagnostics/std)
│   ├── guides/                # Human guide + AI guide (usage & maintenance)
│   ├── bench/                 # All M2–M6 benchmark & experiment reports
│   └── adr/                   # Architecture decision records
├── crates/                    # Rust workspace (sub-projects)
│   ├── aether-ast/            # AST / NodeId / Span / canonical printing
│   ├── aether-diagnostic/     # Structured diagnostics (JSON)
│   ├── aether-parser/         # Lexer + recursive-descent parser
│   ├── aether-verify/         # Type checker + Z3 static contract verification
│   ├── aether-interp/         # Tree-walking interpreter (value model / builtins)
│   ├── aether-vm/             # Bytecode compiler + VM (default backend)
│   └── aether-cli/            # parse / check / run / repl
├── lib/aether-std.ae          # Standard library written in Aether itself (self-hosting)
├── examples/                  # Aether examples (incl. tank battle, animated art text)
└── tools/harness/             # AI generation loop, benchmarks, distillation experiments
```

## Current Status

**MVP achieved: AI-generated Aether code → parse → type check → Z3 static contract verification → bytecode VM execution — the full pipeline runs.**

| Milestone | Status | Key artifacts |
| :--- | :--- | :--- |
| M0 Project setup | ✅ | Vision / roadmap / architecture / agent manual |
| M1 Syntax & parsing | ✅ | spec v0.1 + lexer/parser/printer/diagnostics |
| M2 Interpreter MVP | ✅ | Tree-walking interpreter + 40+ builtins |
| M3 Types & contract verification | ✅ | Bidirectional type checking + Z3 static verification (counterexamples) |
| M4 AI generation loop | ✅ | Benchmark + distillation loop: pass rate 15%→55% (+40pp), tokens −55% |
| M5 Performance backend | ✅ | Bytecode VM (same order of magnitude as CPython, default backend) |
| M6 Self-hosting & static distillation | ✅ | Aether std library self-hosted + graph patching + distillation experiment |
| M7+ | 💭 Vision | Intent marketplace / hardware adaptation (unscheduled) |

> See [docs/roadmap.md](docs/roadmap.md) and the reports under [docs/bench/](docs/bench/).

## Quick Start

```powershell
cargo build --release
cargo run -p aether-cli -- run examples/hello.ae                # run a program (prints Hello, Aether!)
cargo run -p aether-cli -- check examples/bounds.ae             # type check + Z3 static verification (E4001 demo)
cargo run -p aether-cli -- run lib/aether-std.ae                # self-hosted std library (19 functions)
cargo run -p aether-cli -- parse examples/sort.ae               # parse and dump the AST
cargo run -p aether-cli -- repl                                 # interactive evaluation
cargo run -p aether-vm --example graph_patch                    # M6 graph-patch / hot-update demo
cargo test                                                      # 148 tests

# Natural language → AI generation → verification → execution (end-to-end; needs local Ollama):
python tools/harness/demo_nl.py "写一个斐波那契函数,带前置条件契约" --input 10

# M4/M6 generation-loop benchmarks (needs local Ollama; default model gemma3:4b):
python tools/harness/harness.py --tasks fib,gcd                # omit --tasks to run all
python tools/harness/harness.py --corpus lib/aether-std.ae     # static distillation mode
```

## How to Work With This Project

- **Humans** (intent architects / reviewers / maintainers): read [docs/guides/human-guide.md](docs/guides/human-guide.md)
- **AI** (generators / maintainers): read [docs/guides/ai-guide.md](docs/guides/ai-guide.md)
- All AI agents working on this repo must read [AGENTS.md](AGENTS.md) first.

## License

[MIT](LICENSE)

## Background Material

Original design conversations and survey: [AI跨语言设计.txt](./AI跨语言设计.txt) (read-only archive — do not modify).
