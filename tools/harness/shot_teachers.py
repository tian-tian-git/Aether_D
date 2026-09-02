# -*- coding: utf-8 -*-
"""验证 teachers2.html:JS 无错 + 文字区域亮色像素统计(艺术字渲染证据)。"""
from pathlib import Path

from playwright.sync_api import sync_playwright

PAGE = Path(r"D:\Desktop\programme\ai-lp\Aether_D\target\teachers2.html").as_uri()

JS = """
() => {
  const cv = document.getElementById('c');
  const g = cv.getContext('2d');
  const W = 720, H = 480;
  // 文字带:y 110..240(艺术字可能滑入/波动);粒子散布全屏
  const text = g.getImageData(60, 100, 600, 150).data;   // 文字区域
  const full = g.getImageData(0, 0, W, H).data;          // 全屏
  let brightText = 0, brightFull = 0;
  for (let i = 0; i < text.length; i += 4) {
    const r = text[i], gg = text[i+1], b = text[i+2];
    if (r + gg + b > 300) brightText++;
  }
  for (let i = 0; i < full.length; i += 4) {
    if (full[i] + full[i+1] + full[i+2] > 300) brightFull++;
  }
  return { brightText, brightFull };
}
"""

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page(viewport={"width": 800, "height": 620})
    errors = []
    page.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.goto(PAGE)
    page.wait_for_load_state("networkidle")
    page.wait_for_timeout(2200)  # 入场滑落进行中/完成
    stats = page.evaluate(JS)
    print("console errors:", errors if errors else "无")
    print("文字区域亮色像素:", stats["brightText"], "(>2000 即艺术字已渲染)")
    print("全屏亮色像素:", stats["brightFull"])
    page.screenshot(path=r"D:\Desktop\programme\ai-lp\Aether_D\target\teachers-shot.png")
    browser.close()
