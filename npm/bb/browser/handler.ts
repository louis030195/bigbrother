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
  const { page } = session;

  switch (method) {
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
        await page.click(`text=${text}`, { timeout });
        return { clicked: { text } };
      }

      if (role) {
        const opts: Record<string, unknown> = {};
        if (name) opts.name = new RegExp(name, "i");
        const el = page.getByRole(role as any, opts).first();
        await el.scrollIntoViewIfNeeded();
        await el.click({ timeout });
        return { clicked: { role, name } };
      }

      if (selector) {
        await page.click(selector, { timeout });
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
        if (name) opts.name = new RegExp(name, "i");
        const el = page.getByRole(role as any, opts).first();
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
        await page.fill(selector, text);
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
      const path = (params.path as string) || undefined;
      const buffer = await page.screenshot({
        path,
        fullPage: (params.fullPage as boolean) || false,
      });
      return {
        path: path || null,
        size: buffer.length,
        base64: path ? undefined : buffer.toString("base64"),
      };
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
      const text = await page.evaluate(() => {
        const remove = document.querySelectorAll("script, style, noscript, svg, img, link, meta");
        remove.forEach((el) => el.remove());
        return document.body?.innerText || "";
      });
      return { text, url: page.url(), title: await page.title() };
    }

    case "snapshot": {
      const selector = (params.selector as string) || "body";
      const timeout = (params.timeout as number) || 10000;
      const locator = page.locator(selector);
      const snapshot = await locator.ariaSnapshot({ timeout });
      return { snapshot, url: page.url() };
    }

    case "find": {
      const role = requireParam<string>(params, "role");
      const name = params.name as string | undefined;
      const opts: Record<string, unknown> = {};
      if (name) opts.name = new RegExp(name, "i");

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
        if (name) opts.name = new RegExp(name, "i");
        await page.getByRole(role as any, opts).first().waitFor({ timeout });
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
        `Unknown method: ${method}. Available: navigate, click, type, press, screenshot, evaluate, content, text, snapshot, find, find_placeholder, url, wait, scroll, cookies, close`
      );
  }
}

export function requireParam<T>(params: Record<string, unknown>, name: string): T {
  if (!(name in params)) {
    throw new Error(`Missing required parameter: ${name}`);
  }
  return params[name] as T;
}
