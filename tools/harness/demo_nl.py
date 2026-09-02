# -*- coding: utf-8 -*-
"""demo_nl.py —— 项目愿景演示:自然语言 → LLM 生成 Aether → 验证 → 执行。

用法:
    python tools/harness/demo_nl.py "写一个函数 fib(n),返回第 n 个斐波那契数" [--input "10"]

流程:
    1. 用自然语言描述需求(一行);
    2. LLM 生成 Aether 函数(harness 同一套提示与修补回路);
    3. parse → typecheck + Z3 静态契约验证 → 全通过后执行样例输入,打印结果。
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness  # noqa: E402

FN_NAME = "demo-fn"


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if not args:
        print(__doc__)
        return 2
    prompt = args[0]
    sample = args[1] if len(args) > 1 else None

    harness._apply_argv()
    sys_prompt = harness.AETHER_SYS
    msg = prompt + f"\nRequired function name: {FN_NAME}\n"
    code = ""
    for attempt in range(1, harness.MAX_ROUNDS + 1):
        raw, p, c = harness.ollama_chat(sys_prompt, msg)
        code = harness.strip_fences(raw)
        print(f"--- 第 {attempt} 轮生成(tokens {p}/{c})---")
        print(code)
        # 验证:parse + check(静态契约)
        f = harness.TMP / "demo.ae"
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text("(module t\n" + code + "\n)\n", encoding="utf-8")
        import subprocess
        r = subprocess.run([str(harness.AETHER), "check", str(f), "--json"],
                           capture_output=True, text=True, timeout=120)
        if r.returncode == 0:
            break
        print("--- 验证失败,反馈回喂 ---")
        msg = ("Task: " + prompt + f"\nRequired function name: {FN_NAME}\n"
               + "Your previous solution failed validation. Feedback:\n"
               + (r.stdout or r.stderr).strip()
               + "\nFix ALL errors and respond with the complete corrected code only.")
    else:
        print("生成失败:修补轮数耗尽")
        return 1

    print("--- 验证通过:parse + typecheck + 静态契约验证 全绿 ---")
    if sample is not None:
        lit = harness.aether_lit(int(sample) if sample.lstrip("-").isdigit() else sample)
        runner = f"(module t\n{code}\n(fn main () -> Int (block (out ({FN_NAME} {lit})) 0)))\n"
        fr = harness.TMP / "demo-run.ae"
        fr.write_text(runner, encoding="utf-8")
        import subprocess
        r = subprocess.run([str(harness.AETHER), "run", str(fr)],
                           capture_output=True, text=True, timeout=60)
        print("--- 执行结果 ---")
        print(r.stdout or r.stderr)
        return r.returncode
    return 0


if __name__ == "__main__":
    sys.exit(main())
