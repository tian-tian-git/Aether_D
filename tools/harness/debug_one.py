# -*- coding: utf-8 -*-
"""调试:单任务生成 + 逐轮验证,打印诊断反馈。"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness  # noqa: E402
from tasks import TASKS  # noqa: E402

t = [x for x in TASKS if x["id"] == "fib"][0]
sys_prompt = harness.AETHER_SYS
msg = t["prompt"] + f"\nRequired function name: {harness.AETHER_FN.get(t['id'], t['id'])}\n"
for attempt in range(1, 4):
    code_raw, p, c = harness.ollama_chat(sys_prompt, msg)
    code = harness.strip_fences(code_raw)
    print(f"===== round {attempt} (tokens {p}/{c}) =====")
    print(code)
    ok, fb = harness.aether_validate(code, "fib", t["cases"])
    print(f"----- ok={ok} feedback -----")
    print(fb[:1500])
    if ok:
        break
    msg = ("Your previous solution failed validation. Here is the feedback:\n"
           + fb + "\nFix ALL errors and respond with the complete corrected code only.")
