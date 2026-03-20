import { describe, it } from "node:test";
import * as assert from "node:assert";
import { launch } from "../launcher.js";
import { findProfile } from "../profiles/index.js";

describe("launcher", () => {
  it("launches headless browser with cookies and closes cleanly", async () => {
    const profile = findProfile("chrome") || findProfile("arc");
    if (!profile) {
      console.log("  (skipped: no Chrome/Arc profile found)");
      return;
    }

    const session = await launch({
      browser: profile.browser,
      headless: true,
      url: "https://example.com",
    });

    try {
      assert.ok(session.browser, "should have browser instance");
      assert.ok(session.context, "should have context");
      assert.ok(session.page, "should have page");

      const title = await session.page.title();
      assert.ok(typeof title === "string", "should get page title");

      const url = session.page.url();
      assert.ok(url.includes("example.com"), "should be on example.com");

      // Verify cookies were injected and stats are reported
      const cookies = await session.context.cookies();
      assert.ok(cookies.length > 0, "should have cookies in context");

      assert.ok(session.cookieStats, "should have cookieStats");
      assert.ok(session.cookieStats.total > 0, "should report total cookies");
      assert.ok(session.cookieStats.injected > 0, "should report injected count");
      assert.ok(typeof session.cookieStats.skipped === "number", "should report skipped count");
      assert.ok(Array.isArray(session.cookieStats.errors), "should have errors array");
    } finally {
      await session.cleanup();
    }
  });

  it("throws for nonexistent browser profile", async () => {
    await assert.rejects(
      () => launch({ browser: "nonexistent_xyz", headless: true }),
      /No nonexistent_xyz profile found/
    );
  });
});
