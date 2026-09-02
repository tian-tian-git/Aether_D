# -*- coding: utf-8 -*-
"""同题 A/B(含一轮修补):Aether 首轮失败后按诊断修补一次。"""
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


if __name__ == "__main__":
    harness._apply_argv()
    name_line = "\nRequired function name: page\n"
    msg = PROMPT + name_line
    code, p1, c1 = harness.ollama_chat(harness.AETHER_SYS, msg)
    code = harness.strip_fences(code)
    ok, fb = aether_valid(code)
    if not ok:
        msg2 = ("Task: " + PROMPT + name_line
                + "Your previous solution failed validation. Feedback:\n" + fb
                + "\nFix ALL errors and respond with the complete corrected code only.")
        code2, p2, c2 = harness.ollama_chat(harness.AETHER_SYS, msg2)
        code2 = harness.strip_fences(code2)
        ok2, fb2 = aether_valid(code2)
        print(f"aether: first={ok} repaired={ok2} tokens_total={p1 + p2}/{c1 + c2} code_len={len(code2)}")
        if fb2:
            print("feedback2:", fb2)
    else:
        print(f"aether: first=True tokens={p1}/{c1} code_len={len(code)}")
