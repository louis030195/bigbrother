// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
import { runtimeFor } from "./runtime.js";
import type { BrowserSession } from "./launcher.js";

/**
 * Handle a single browser command. Shared between stdin/stdout server,
 * Unix socket session daemon, and one-shot CLI commands.
 */
export async function handleMethod(
  session: BrowserSession,
  method: string,
  params: Record<string, unknown>
): Promise<unknown> {
  const runtime = runtimeFor(session.page);
  return runtime.run(() => handleCommand(session, method, params));
}

async function handleCommand(session: BrowserSession, method: string, params: Record<string, unknown>): Promise<unknown> {
  const { page } = session;
  const runtime = runtimeFor(page);
  if (page.isClosed()) throw new Error("Browser session is closed");
  if (["navigate", "click", "type", "press", "evaluate", "scroll", "close"].includes(method)) runtime.invalidate();
  switch (method) {
    case "capabilities": return { protocol: 1, source: "chromium_ax_dom", observe: true, diffs: true, element_actions: ["click", "set_value"], actions_require_revision: true, screenshot_coordinates: "viewport_css_pixels" };
    case "observe": return runtime.observe(params);
    case "act": return runtime.act(params as any);
    case "click_at": return runtime.clickAt(params as any);
    case "navigate": {
      const url = requireParam<string>(params, "url");
      await page.goto(url, { waitUntil: "domcontentloaded" });
      return { url: page.url(), title: await page.title() };
    }

    case "click": {
      const selector = params.selector as string | undefined;
      const text = params.text as string | undefined;
      const role = params.role as string | undefined;
      const name = params.name as string | undefined;
      const timeout = (params.timeout as number) || 5000;

      if (text) {
        await page.getByText(text, { exact: true }).click({ timeout });
        return { clicked: { text } };
      }

      if (role) {
        const opts: Record<string, unknown> = {};
        if (name) { opts.name = name; opts.exact = true; }
        const el = page.getByRole(role as any, opts);
        await el.scrollIntoViewIfNeeded();
        await el.click({ timeout });
        return { clicked: { role, name } };
      }

      if (selector) {
        await page.locator(selector).click({ timeout });
        return { clicked: selector };
      }

      throw new Error("Provide 'text', 'selector', or 'role' (+optional 'name')");
    }

    case "type": {
      const text = requireParam<string>(params, "text");
      const selector = params.selector as string | undefined;
      const role = params.role as string | undefined;
      const name = params.name as string | undefined;
      const humanlike = params.humanlike as boolean | undefined;

      if (role) {
        const opts: Record<string, unknown> = {};
        if (name) { opts.name = name; opts.exact = true; }
        const el = page.getByRole(role as any, opts);
        await el.scrollIntoViewIfNeeded();
        await el.click();
        if (humanlike) {
          await page.keyboard.type(text, { delay: 30 + Math.random() * 50 });
        } else {
          await el.fill(text);
        }
        return { typed: text, target: { role, name } };
      }

      if (selector) {
        await page.locator(selector).fill(text);
        return { typed: text, selector };
      }

      // No target — type into currently focused element
      if (humanlike) {
        await page.keyboard.type(text, { delay: 30 + Math.random() * 50 });
      } else {
        await page.keyboard.insertText(text);
      }
      return { typed: text, target: "focused" };
    }

    case "press": {
      const key = requireParam<string>(params, "key");
      await page.keyboard.press(key);
      return { pressed: key };
    }

    case "screenshot": {
      const path = params.path as string | undefined;
      if (params.fullPage) {
        runtime.invalidate();
        const buffer = await page.screenshot({ path, fullPage: true });
        return { path: path ?? null, base64: path ? undefined : buffer.toString("base64"), coordinate_input: false };
      }
      return runtime.screenshot(path);
    }

    case "evaluate": {
      const expression = requireParam<string>(params, "expression");
      const result = await page.evaluate(expression);
      return { result };
    }

    case "content": {
      return { html: await page.content() };
    }

    case "text": {
      const text = await page.locator("body").innerText();
      const max = typeof params.maxChars === "number" ? params.maxChars : 32000;
      if (!Number.isInteger(max) || max < 1 || max > 1000000) throw new Error("Invalid maxChars");
      const chars = Array.from(text);
      return { text: chars.slice(0, max).join(""), truncated: chars.length > max, url: page.url(), title: await page.title() };
    }

    case "snapshot": {
      if (params.selector || params.raw) {
        runtime.invalidate();
        return { snapshot: await page.locator((params.selector as string) || "body").ariaSnapshot({ timeout: 10000 }), source: "playwright_aria", actionable_references: false, url: page.url() };
      }
      return runtime.observe(params);
    }

    case "find": {
      const role = requireParam<string>(params, "role");
      const name = params.name as string | undefined;
      const opts: Record<string, unknown> = {};
      if (name) { opts.name = name; opts.exact = true; }

      const elements = await page.getByRole(role as any, opts).all();
      const results = [];
      for (let i = 0; i < Math.min(elements.length, 50); i++) {
        const el = elements[i];
        const text = await el.textContent().catch(() => "") ?? "";
        const label = await el.getAttribute("aria-label").catch(() => "") ?? "";
        const href = await el.getAttribute("href").catch(() => "") ?? "";
        const visible = await el.isVisible().catch(() => false);
        if (!visible) continue;
        results.push({
          index: i,
          text: text.trim().slice(0, 200),
          label,
          href,
        });
      }
      return { role, name, count: results.length, elements: results };
    }

    case "find_placeholder": {
      const text = requireParam<string>(params, "text");
      const elements = await page.getByPlaceholder(new RegExp(text, "i")).all();
      const results = [];
      for (const el of elements) {
        const ph = await el.getAttribute("placeholder").catch(() => "") ?? "";
        const visible = await el.isVisible().catch(() => false);
        results.push({ placeholder: ph, visible });
      }
      return { count: results.length, elements: results };
    }

    case "url": {
      return { url: page.url(), title: await page.title() };
    }

    case "wait": {
      const selector = params.selector as string | undefined;
      const role = params.role as string | undefined;
      const name = params.name as string | undefined;
      const timeout = (params.timeout as number) || 5000;

      if (role) {
        const opts: Record<string, unknown> = {};
        if (name) { opts.name = name; opts.exact = true; }
        await page.getByRole(role as any, opts).waitFor({ timeout });
        return { found: { role, name } };
      }

      if (selector) {
        await page.waitForSelector(selector, { timeout });
        return { found: selector };
      }
      const ms = (params.ms as number) || 1000;
      await page.waitForTimeout(ms);
      return { waited: ms };
    }

    case "scroll": {
      const direction = (params.direction as string) || "down";
      const amount = (params.amount as number) || 500;
      const delta = direction === "up" ? -amount : amount;
      await page.mouse.wheel(0, delta);
      return { direction, amount };
    }

    case "cookies": {
      const cookies = await session.context.cookies();
      return { cookies, count: cookies.length };
    }

    case "close": {
      await session.cleanup();
      return { closed: true };
    }

    default:
      throw new Error(
        `Unknown method: ${method}. Available: capabilities, observe, act, click_at, navigate, click, type, press, screenshot, evaluate, content, text, snapshot, find, find_placeholder, url, wait, scroll, cookies, close`
      );
  }
}

export function requireParam<T>(params: Record<string, unknown>, name: string): T {
  if (!(name in params)) {
    throw new Error(`Missing required parameter: ${name}`);
  }
  return params[name] as T;
}
