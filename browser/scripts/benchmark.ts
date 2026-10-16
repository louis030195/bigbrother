// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
import { chromium } from "playwright";
import { BrowserRuntime } from "../src/runtime.js";
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  await page.setContent(
    Array.from(
      { length: 300 },
      (_, i) => `<button id="b${i}">Action ${i}</button>`,
    ).join(""),
  );
  const cdp = await page.context().newCDPSession(page);
  const raw = await cdp.send("Accessibility.getFullAXTree");
  await cdp.detach();
  const runtime = new BrowserRuntime(page);
  const full = await runtime.observe({ limits: { maxChildren: 1000 } });
  const unchanged = await runtime.observe({ limits: { maxChildren: 1000 } });
  await page.locator("#b0").evaluate((el) => {
    el.textContent = "Saved";
  });
  const changed = await runtime.observe({ limits: { maxChildren: 1000 } });
  console.log(
    JSON.stringify(
      {
        fixture: "300 buttons in isolated Chromium",
        raw_ax_json_bytes: Buffer.byteLength(JSON.stringify(raw)),
        rendered_text_bytes: Buffer.byteLength(full.text),
        rendered_nodes: full.node_count,
        unchanged_text_bytes: Buffer.byteLength(unchanged.text),
        unchanged_mode: unchanged.mode,
        changed_text_bytes: Buffer.byteLength(changed.text),
        changed_mode: changed.mode,
        truncated: full.truncated,
        scope: "Byte counts, not model token counts or production task success",
      },
      null,
      2,
    ),
  );
} finally {
  await browser.close();
}
