// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
import { test, expect, beforeAll, afterAll } from "bun:test";
import { chromium, type Browser, type Page } from "playwright";
import { BrowserRuntime } from "../runtime.js";
import { handleMethod } from "../handler.js";
import type { BrowserSession } from "../launcher.js";
let browser: Browser;
beforeAll(async () => {
  browser = await chromium.launch({ headless: true });
});
afterAll(async () => {
  await browser?.close();
});
async function fixture(
  html: string,
  run: (page: Page, runtime: BrowserRuntime) => Promise<void>,
) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    await page.setContent(html);
    await run(page, new BrowserRuntime(page));
  } finally {
    await context.close();
  }
}
function ref(text: string, role: string, name: string): string {
  const line = text
    .split("\n")
    .find((l) => l.includes(`${role} ${JSON.stringify(name)}`));
  if (!line) throw new Error(`Missing ${role} ${name} in ${text}`);
  return line.match(/\[(e\d+)\]/)![1];
}
test("real AX/DOM capture, protected value, exact action, revision invalidation", async () => {
  await fixture(
    '<input aria-label="Secret" type="password" value="do-not-leak"><button onclick="this.textContent=\'Saved\'">Save</button>',
    async (page, runtime) => {
      const first = await runtime.observe();
      expect(first.source).toBe("chromium_ax_dom");
      expect(first.text).not.toContain("do-not-leak");
      const unchanged = await runtime.observe();
      expect(unchanged.mode).toBe("unchanged");
      const reference = ref(
        unchanged.text === "[unchanged]" ? first.text : unchanged.text,
        "button",
        "Save",
      );
      await runtime.act({
        revision: unchanged.revision,
        reference,
        action: "click",
      });
      expect(await page.getByRole("button").textContent()).toBe("Saved");
      await expect(
        runtime.act({
          revision: unchanged.revision,
          reference,
          action: "click",
        }),
      ).rejects.toThrow("STALE_OBSERVATION");
    },
  );
});
test("replaced and relabelled controls are rejected before click", async () => {
  await fixture(
    '<button id="target" onclick="window.clicked=true">Save</button>',
    async (page, runtime) => {
      const first = await runtime.observe();
      const reference = ref(first.text, "button", "Save");
      await page.evaluate(() => {
        document.querySelector("button")!.textContent = "Delete";
      });
      await expect(
        runtime.act({ revision: first.revision, reference, action: "click" }),
      ).rejects.toThrow("STALE_OBSERVATION");
      expect(
        await page.evaluate(() => (window as any).clicked),
      ).toBeUndefined();
    },
  );
});
test("iframe references resolve to their own frame and fill through Playwright", async () => {
  await fixture(
    '<iframe srcdoc="<input aria-label=Name>"></iframe><input aria-label="Name">',
    async (page, runtime) => {
      await page.frames()[1].waitForLoadState();
      const first = await runtime.observe();
      const fields = first.text
        .split("\n")
        .filter((l) => l.includes('textbox "Name"'));
      expect(fields.length).toBe(2);
      const reference = fields[1].match(/\[(e\d+)\]/)![1];
      await runtime.act({
        revision: first.revision,
        reference,
        action: "set_value",
        value: "你好🙂",
      });
      expect(await page.frames()[1].getByRole("textbox").inputValue()).toBe(
        "你好🙂",
      );
      expect(await page.getByRole("textbox").inputValue()).toBe("");
    },
  );
});
test("screenshots bind coordinates to unchanged viewport pixels", async () => {
  await fixture(
    "<button onclick=\"this.textContent='Clicked'\">Save</button>",
    async (page, runtime) => {
      const shot = await runtime.screenshot();
      await page.evaluate(() => {
        document.body.style.background = "red";
      });
      await expect(
        runtime.clickAt({ screenshot_id: shot.screenshot_id, x: 20, y: 15 }),
      ).rejects.toThrow("STALE_SCREENSHOT");
      const fresh = await runtime.screenshot();
      await runtime.clickAt({
        screenshot_id: fresh.screenshot_id,
        x: 20,
        y: 15,
      });
      expect(await page.getByRole("button").textContent()).toBe("Clicked");
    },
  );
});
test("text observation preserves the DOM and ambiguous locators fail", async () => {
  await fixture(
    "<style>button{color:red}</style><button>Save</button><button>Save</button><script>window.marker=1</script>",
    async (page) => {
      const session = {
        page,
        context: page.context(),
        cleanup: async () => {},
      } as BrowserSession;
      await handleMethod(session, "text", {});
      expect(await page.locator("style").count()).toBe(1);
      expect(await page.locator("script").count()).toBe(1);
      await expect(
        handleMethod(session, "click", {
          role: "button",
          name: "Save",
          timeout: 100,
        }),
      ).rejects.toThrow("strict mode");
    },
  );
});
test("session queue drains in order even when a request fails", async () => {
  await fixture("<button>Save</button>", async (_page, runtime) => {
    const order: number[] = [];
    const a = runtime
      .run(async () => {
        await new Promise((r) => setTimeout(r, 10));
        order.push(1);
        throw new Error("expected");
      })
      .catch(() => {});
    const b = runtime.run(async () => {
      order.push(2);
    });
    await Promise.all([a, b]);
    expect(order).toEqual([1, 2]);
  });
});

test("out-of-process cross-site iframe actions use the matching CDP target", async () => {
  const server = Bun.serve({
    hostname: "0.0.0.0",
    port: 0,
    fetch(req) {
      const url = new URL(req.url);
      return new Response(
        url.pathname === "/child"
          ? "<button onclick=\"this.textContent='Done'\">Child action</button>"
          : `<iframe src="http://127.0.0.1:${url.port}/child"></iframe>`,
        { headers: { "content-type": "text/html" } },
      );
    },
  });
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    await page.goto(`http://localhost:${server.port}/`);
    const runtime = new BrowserRuntime(page);
    const state = await runtime.observe();
    const reference = ref(state.text, "button", "Child action");
    await runtime.act({ revision: state.revision, reference, action: "click" });
    expect(await page.frames()[1].getByRole("button").textContent()).toBe(
      "Done",
    );
  } finally {
    await context.close();
    server.stop(true);
  }
});

test("node budget is explicit and navigation invalidates old references", async () => {
  await fixture("<button>Save</button>".repeat(20), async (page, runtime) => {
    const limited = await runtime.observe({ limits: { maxNodes: 5 } });
    expect(limited.truncated).toBe(true);
    expect(limited.node_count).toBeLessThanOrEqual(5);
    const state = await runtime.observe();
    const reference = ref(state.text, "button", "Save");
    await page.goto("data:text/html,<button>Save</button>");
    await expect(
      runtime.act({ revision: state.revision, reference, action: "click" }),
    ).rejects.toThrow("STALE_OBSERVATION");
  });
});
