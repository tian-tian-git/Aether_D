# -*- coding: utf-8 -*-
"""调试指定任务的首轮生成与验证反馈。"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness  # noqa: E402
from tasks import TASKS  # noqa: E402

tid = sys.argv[1]
t = [x for x in TASKS if x["id"] == tid][0]
fn = harness.AETHER_FN.get(tid, tid)
msg = t["prompt"] + f"\nRequired function name: {fn}\n"
code_raw, p, c = harness.ollama_chat(harness.AETHER_SYS, msg)
code = harness.strip_fences(code_raw)
print("===== round 1 =====")
print(code)
ok, fb = harness.aether_validate(code, fn, t["cases"])
print("===== ok =", ok)
print(fb[:1200])
