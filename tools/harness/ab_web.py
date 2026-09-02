# -*- coding: utf-8 -*-
"""同题 A/B:同一自然语言需求(生成 HTML 页面的函数),Aether vs Python 各生成一次。
有效性口径:一次生成 → Aether: parse+typecheck+静态验证;Python: py_compile。"""
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness  # noqa: E402

PROMPT = "写一个函数 page(title),返回一段完整 HTML 网页字符串,包含 <h1>Hello World</h1> 和传入的 title。"


def aether_valid(code: str):
    f = harness.TMP / "sub.ae"
    f.parent.mkdir(parents=True, exist_ok=True)
    f.write_text("(module t\n" + code + "\n)\n", encoding="utf-8")
    r = subprocess.run([str(harness.AETHER), "check", str(f), "--json"],
                       capture_output=True, text=True, timeout=60)
    return r.returncode == 0 and '"ok":true' in r.stdout, (r.stdout or r.stderr)[:240]


def python_valid(code: str):
    f = harness.TMP / "sub.py"
    f.parent.mkdir(parents=True, exist_ok=True)
    f.write_text(code, encoding="utf-8")
    r = subprocess.run([sys.executable, "-m", "py_compile", str(f)],
                       capture_output=True, text=True, timeout=30)
    return r.returncode == 0, (r.stderr or "")[:240]


def run_one(system, msg, validator):
    code, p_tok, c_tok = harness.ollama_chat(system, msg)
    code = harness.strip_fences(code)
    ok, fb = validator(code)
    return {"prompt_tokens": p_tok, "completion_tokens": c_tok, "valid": ok,
            "code_len": len(code), "feedback": fb}


if __name__ == "__main__":
    harness._apply_argv()
    name_line = "\nRequired function name: page\n"
    a = run_one(harness.AETHER_SYS, PROMPT + name_line, aether_valid)
    p = run_one(harness.PY_SYS, PROMPT + name_line, python_valid)
    print(f"aether : valid={a['valid']}  tokens={a['prompt_tokens']}/{a['completion_tokens']}  code_len={a['code_len']}")
    print(f"python : valid={p['valid']}  tokens={p['prompt_tokens']}/{p['completion_tokens']}  code_len={p['code_len']}")
    if a["feedback"]:
        print("aether feedback:", a["feedback"])
    if p["feedback"]:
        print("python feedback:", p["feedback"])
