# -*- coding: utf-8 -*-
"""自检:任务集的 Aether 参考实现必须 parse+check 通过。"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
AETHER = ROOT / "target" / "release" / "aether-cli.exe"
sys.path.insert(0, str(Path(__file__).resolve().parent))
from tasks import TASKS  # noqa: E402

bad = 0
for t in TASKS:
    src = "(module t\n" + t["aether_ref"] + "\n)\n"
    f = ROOT / "target" / "harness-tmp" / f"self-{t['id']}.ae"
    f.parent.mkdir(parents=True, exist_ok=True)
    f.write_text(src, encoding="utf-8")
    r = subprocess.run([str(AETHER), "check", str(f), "--json"], capture_output=True, text=True, timeout=60)
    ok = r.returncode == 0 and '"ok":true' in r.stdout
    print(f"{'OK ' if ok else 'BAD'} {t['id']}  {r.stdout[:80] if not ok else ''}")
    if not ok:
        bad += 1
sys.exit(1 if bad else 0)
