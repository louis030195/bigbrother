import { describe, it } from "node:test";
import * as assert from "node:assert";
import * as fs from "fs";
import { findProfile } from "../profiles/index.js";
import { extractCookies } from "../cookies.js";

describe("cookies", () => {
  it("extractCookies returns array from real profile", () => {
    const profile = findProfile("chrome") || findProfile("arc");
    if (!profile) {
      console.log("  (skipped: no Chrome/Arc profile found)");
      return;
    }

    const cookies = extractCookies(profile.cookiesPath, profile.browser);
    assert.ok(Array.isArray(cookies));
    assert.ok(cookies.length > 0, "should extract at least one cookie");
  });

  it("extracted cookies have valid structure", () => {
    const profile = findProfile("chrome") || findProfile("arc");
    if (!profile) {
      console.log("  (skipped: no Chrome/Arc profile found)");
      return;
    }

    const cookies = extractCookies(profile.cookiesPath, profile.browser);
    for (const c of cookies.slice(0, 5)) {
      assert.ok(typeof c.name === "string", "cookie should have name");
      assert.ok(typeof c.value === "string", "cookie should have value");
      assert.ok(typeof c.domain === "string", "cookie should have domain");
      assert.ok(typeof c.path === "string", "cookie should have path");
      assert.ok(typeof c.secure === "boolean", "cookie should have secure");
      assert.ok(typeof c.httpOnly === "boolean", "cookie should have httpOnly");
    }
  });

  it("domain filter works", () => {
    const profile = findProfile("chrome") || findProfile("arc");
    if (!profile) {
      console.log("  (skipped: no Chrome/Arc profile found)");
      return;
    }

    const all = extractCookies(profile.cookiesPath, profile.browser);
    const filtered = extractCookies(
      profile.cookiesPath,
      profile.browser,
      "google.com"
    );

    assert.ok(filtered.length <= all.length, "filtered should be <= all");
    for (const c of filtered) {
      assert.ok(
        c.domain.includes("google.com"),
        `cookie domain should contain google.com: ${c.domain}`
      );
    }
  });
});
