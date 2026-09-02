# -*- coding: utf-8 -*-
"""M4 生成回路 harness:LLM 生成 → 验证 → 结构化诊断回喂 → 修补循环 → 指标。

用法:
    python tools/harness/harness.py [--tasks fib,gcd,...] [--rounds 3] [--model gemma3:4b]

流程(每种语言 × 每任务):
    1. 用系统提示(含语言语法参考)向本地 Ollama 请求函数实现;
    2. Aether: parse --json → check --json(类型 + Z3 静态契约)→ 测试运行(run);
       Python: py_compile → 测试运行(pytest 风格 assert,超时保护);
    3. 失败 → 把结构化诊断回喂 LLM 修补(最多 N 轮);
    4. 记录:轮数、一次通过、最终通过、prompt/completion token 数。
输出:docs/bench/m4-report.md(对比报告)。
"""

import json
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

from tasks import TASKS

ROOT = Path(__file__).resolve().parent.parent.parent
AETHER = ROOT / "target" / "release" / "aether-cli.exe"
TMP = ROOT / "target" / "harness-tmp"
MODEL = "gemma3:4b"
MAX_ROUNDS = 3


def _argv_opt(name: str, default: str):
    if name in sys.argv:
        i = sys.argv.index(name)
        if i + 1 < len(sys.argv):
            return sys.argv[i + 1]
    return default


def _apply_argv() -> None:
    global MODEL, MAX_ROUNDS
    MODEL = _argv_opt("--model", MODEL)
    MAX_ROUNDS = int(_argv_opt("--rounds", str(MAX_ROUNDS)))

# 函数名映射(kebab → 各自语言)
AETHER_FN = {"triangle": "is-triangle", "celsius": "celsius-to-f", "sorted-insert": "insert", "binary-search": "bsearch"}
PY_FN = {
    "fib": "fib", "fact": "fact", "gcd": "gcd", "sum": "sum", "max-elem": "max_elem",
    "filter-even": "filter_even", "map-square": "map_square", "reverse": "reverse",
    "is-prime": "is_prime", "contains": "contains", "count": "count", "dot": "dot",
    "sorted-insert": "insert", "qsort": "qsort", "binary-search": "bsearch",
    "hanoi": "hanoi", "power": "power", "digit-sum": "digit_sum",
    "celsius": "celsius_to_f", "triangle": "is_triangle",
}

AETHER_SYS = """You are an expert Aether programmer. Aether is an AI-native S-expression language.
Respond with ONLY the Aether code (function definitions). No markdown fences, no explanations.

Aether syntax reference (S-expressions, everything is parenthesized prefix form):

FULL EXAMPLES (follow these patterns exactly):
(fn fib (n Int) -> Int
  :pre (>= n 0)
  (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

(fn factorial (n Int) -> Int
  :pre (>= n 0)
  (if (<= n 1) 1 (* n (factorial (- n 1)))))

(fn evens (xs (Vec Int)) -> (Vec Int)
  (filter (fn (x Int) -> Bool (== (% x 2) 0)) xs))

(fn total (xs (Vec Int)) -> Int
  (fold (fn (acc Int) (x Int) -> Int (+ acc x)) 0 xs))

RULES:
- Function: (fn name (param Type) ... -> RetType [:pre expr] [:post expr] body)
  * :post contracts may reference `result`. Contracts are boolean expressions.
- Binding: (let name Type value). Block: (block e1 e2 ...) — value is last expression.
- Conditional: (if cond then else) — exactly 3 parts.
- Vector literal: (vec 1 2 3). NO loops — use recursion.
- Types: Int, Float, Bool, Str, (Vec Int), (Fn (T) -> U), () for Unit.
- Builtins: + - * / % == != < <= > >= and or not abs min max sqrt
  len head tail get push concat filter map fold range
  empty? finite? sorted? permutation? has? keys put
  some none ok err is-some? is-none? is-ok? is-err? unwrap
  (get xs i) indexes from 0; (fold f init xs) left fold; (filter pred xs); (map f xs); (range lo hi) = [lo, hi).

COMMON MISTAKES TO AVOID:
- NO square brackets [ ] — vectors are (vec 1 2 3).
- NO infix: write (+ a b), (== a b), (< a b), never (a + b) or (a == b).
- Contracts are (:pre expr) AFTER -> Type, never [:pre ...] in brackets.
- Exactly ONE -> per signature. Every param and binding has an explicit type.
- Function body is ONE expression; multiple statements need (block ...).
- Do NOT write (module ...), do NOT write a main function — only the requested function definitions.
"""

PY_SYS = """You are an expert Python programmer. Respond with ONLY the Python code (function definitions only).
No explanations, no markdown fences, no imports, no main block, no tests. Use only the Python standard library."""


def ollama_chat(sys_prompt: str, user_prompt: str) -> tuple:
    """调用 Ollama chat API,返回 (内容, prompt_tokens, completion_tokens)。"""
    body = json.dumps({
        "model": MODEL,
        "stream": False,
        "messages": [
            {"role": "system", "content": sys_prompt},
            {"role": "user", "content": user_prompt},
        ],
        "options": {"temperature": 0.2},
    }).encode("utf-8")
    for attempt in range(2):
        try:
            req = urllib.request.Request(
                "http://localhost:11434/api/chat", data=body,
                headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req, timeout=240) as r:
                d = json.loads(r.read())
            return d["message"]["content"], d.get("prompt_eval_count", 0), d.get("eval_count", 0)
        except Exception as e:  # noqa: BLE001
            if attempt == 1:
                raise
            print(f"  [ollama retry after: {e}]", flush=True)
            time.sleep(5)


def strip_fences(code: str) -> str:
    lines = code.strip().splitlines()
    if lines and lines[0].strip().startswith("```"):
        lines = lines[1:]
        if lines and lines[-1].strip().startswith("```"):
            lines = lines[:-1]
    return "\n".join(lines).strip()


def aether_lit(v) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        s = repr(v)
        if "e" in s:
            m, e = s.split("e")
            m = m if "." in m else m + ".0"
            s = m + "e" + e
        return s if "." in s else s + ".0"
    if isinstance(v, str):
        return '"' + v.replace("\\", "\\\\").replace('"', '\\"') + '"'
    if isinstance(v, (list, tuple)):
        return "(vec " + " ".join(aether_lit(x) for x in v) + ")"
    raise ValueError(f"unsupported case value: {v!r}")


def aether_validate(code: str, fn: str, cases) -> tuple:
    """返回 (通过?, 反馈文本)。"""
    TMP.mkdir(parents=True, exist_ok=True)
    src = "(module t\n" + code + "\n)\n"
    f = TMP / "sub.ae"
    f.write_text(src, encoding="utf-8")
    r = subprocess.run([str(AETHER), "parse", str(f), "--json"], capture_output=True, text=True, timeout=60)
    if r.returncode != 0 or '"ok":true' not in r.stdout:
        return False, (r.stdout or r.stderr).strip()[:4000]
    r = subprocess.run([str(AETHER), "check", str(f), "--json"], capture_output=True, text=True, timeout=120)
    if r.returncode != 0:
        return False, (r.stdout or r.stderr).strip()[:4000]
    # 测试运行器
    checks = ""
    for args, expected in cases:
        call = f"({fn} {' '.join(aether_lit(a) for a in args)})"
        checks = f"(if (== {call} {aether_lit(expected)}) {checks if checks else '0'} 1)"
    runner = f"(module t\n{code}\n(fn main () -> Int {checks}))\n"
    fr = TMP / "runner.ae"
    fr.write_text(runner, encoding="utf-8")
    r = subprocess.run([str(AETHER), "run", str(fr)], capture_output=True, text=True, timeout=60)
    if r.returncode == 0:
        return True, ""
    return False, (r.stderr or r.stdout).strip()[-4000:]


def python_validate(code: str, fn: str, cases) -> tuple:
    TMP.mkdir(parents=True, exist_ok=True)
    tests = []
    for args, expected in cases:
        call = f"{fn}({', '.join(repr(a) for a in args)})"
        tests.append(f"assert {call} == {expected!r}, f'case {args}: got {{{call}!r}}'")
    prog = code + "\n\n" + "\n".join(tests) + "\n"
    f = TMP / "sub.py"
    f.write_text(prog, encoding="utf-8")
    r = subprocess.run([sys.executable, str(f)], capture_output=True, text=True, timeout=30)
    if r.returncode == 0:
        return True, ""
    return False, (r.stderr or r.stdout).strip()[-4000:]


def run_one(task, lang: str) -> dict:
    tid = task["id"]
    if lang == "aether":
        fn = AETHER_FN.get(tid, tid)
        sys_prompt = AETHER_SYS
        name_line = f"\nRequired function name: {fn}\n"
        validate = lambda c: aether_validate(c, fn, task["cases"])
    else:
        fn = PY_FN[tid]
        sys_prompt = PY_SYS
        name_line = f"\nRequired function name: {fn}\n"
        validate = lambda c: python_validate(c, fn, task["cases"])

    user_prompt = task["prompt"] + name_line
    total_p = total_c = 0
    rounds = 0
    code = ""
    first_pass = False
    history = []
    for attempt in range(1, MAX_ROUNDS + 1):
        rounds = attempt
        msg = user_prompt if attempt == 1 else (
            "Task (do not lose sight of it): " + task["prompt"] + name_line
            + "\nYour previous solution failed validation. Here is the feedback:\n"
            + last_feedback
            + "\nRemember: Aether is S-expressions — no brackets [ ], no infix operators, "
              "(:pre expr) goes after -> Type. Fix ALL errors and respond with the complete corrected code only."
        )
        code_raw, p_tok, c_tok = ollama_chat(sys_prompt, msg)
        total_p += p_tok
        total_c += c_tok
        code = strip_fences(code_raw)
        ok, feedback = validate(code)
        history.append({"round": attempt, "ok": ok, "feedback": feedback[:500]})
        if ok:
            if attempt == 1:
                first_pass = True
            return {
                "task": tid, "lang": lang, "rounds": rounds, "first_pass": True,
                "final_pass": True, "prompt_tokens": total_p, "completion_tokens": total_c,
                "history": history,
            }
        last_feedback = feedback
    return {
        "task": tid, "lang": lang, "rounds": rounds, "first_pass": False,
        "final_pass": False, "prompt_tokens": total_p, "completion_tokens": total_c,
        "history": history,
    }


def main() -> int:
    _apply_argv()
    task_filter = None
    if "--tasks" in sys.argv:
        i = sys.argv.index("--tasks")
        task_filter = set(sys.argv[i + 1].split(","))
    results = []
    tasks = [t for t in TASKS if task_filter is None or t["id"] in task_filter]
    for t in tasks:
        for lang in ("aether", "python"):
            print(f"[{t['id']}/{lang}] generating...", flush=True)
            try:
                r = run_one(t, lang)
            except Exception as e:  # noqa: BLE001
                print(f"  ERROR: {e}", flush=True)
                r = {"task": t["id"], "lang": lang, "rounds": 0, "first_pass": False,
                     "final_pass": False, "prompt_tokens": 0, "completion_tokens": 0,
                     "history": [{"round": 0, "ok": False, "feedback": f"harness error: {e}"}]}
            results.append(r)
            print(f"  -> rounds={r['rounds']} first_pass={r['first_pass']} final_pass={r['final_pass']} "
                  f"tokens={r['prompt_tokens']}/{r['completion_tokens']}", flush=True)
    write_report(results)
    return 0


def write_report(results: list) -> None:
    def stat(lang, key):
        vals = [r[key] for r in results if r["lang"] == lang and r["rounds"] > 0]
        return (sum(vals) / len(vals)) if vals else 0.0

    def rate(lang, key):
        vals = [r for r in results if r["lang"] == lang and r["rounds"] > 0]
        ok = sum(1 for r in vals if r[key])
        return (ok / len(vals)) if vals else 0.0

    lines = ["# M4 AI 生成回路:基准报告\n",
             f"> 模型:本地 Ollama `{MODEL}`(temperature 0.2);修补轮数上限 {MAX_ROUNDS};"
             "每任务每语言独立会话。\n",             "> Aether 验证链:parse → typecheck + Z3 静态契约 → 测试运行;"
             "Python 验证链:py_compile(隐式)→ 测试运行。反馈均为结构化诊断(非原始堆栈)。\n"]
    lines.append("\n## 指标汇总\n")
    lines.append("| 指标 | Aether | Python |\n|---|---|---|\n")
    lines.append(f"| 一次通过率(first-pass) | {rate('aether','first_pass'):.0%} | {rate('python','first_pass'):.0%} |\n")
    lines.append(f"| 最终通过率(≤{MAX_ROUNDS} 轮修补) | {rate('aether','final_pass'):.0%} | {rate('python','final_pass'):.0%} |\n")
    lines.append(f"| 平均修补轮数 | {stat('aether','rounds'):.2f} | {stat('python','rounds'):.2f} |\n")
    lines.append(f"| 平均 prompt tokens | {stat('aether','prompt_tokens'):.0f} | {stat('python','prompt_tokens'):.0f} |\n")
    lines.append(f"| 平均 completion tokens | {stat('aether','completion_tokens'):.0f} | {stat('python','completion_tokens'):.0f} |\n")
    lines.append("\n## 逐任务明细\n\n")
    lines.append("| 任务 | Aether 轮/通过 | Python 轮/通过 | Aether tokens | Python tokens |\n|---|---|---|---|---|\n")
    by_task = {}
    for r in results:
        by_task.setdefault(r["task"], {})[r["lang"]] = r
    for tid in [t["id"] for t in TASKS]:
        a = by_task.get(tid, {}).get("aether")
        p = by_task.get(tid, {}).get("python")
        a_s = f"{a['rounds']}/{('✓' if a['final_pass'] else '✗')}" if a else "-"
        p_s = f"{p['rounds']}/{('✓' if p['final_pass'] else '✗')}" if p else "-"
        a_t = f"{a['prompt_tokens']}/{a['completion_tokens']}" if a else "-"
        p_t = f"{p['prompt_tokens']}/{p['completion_tokens']}" if p else "-"
        lines.append(f"| {tid} | {a_s} | {p_s} | {a_t} | {p_t} |\n")
    lines.append("\n## 结论(自动生成,待主智能体解读)\n")
    out = ROOT / "docs" / "bench" / "m4-report.md"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("".join(lines), encoding="utf-8")
    print(f"report written: {out}")


if __name__ == "__main__":
    sys.exit(main())
