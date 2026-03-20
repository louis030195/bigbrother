import { describe, it } from "node:test";
import * as assert from "node:assert";
import { discoverProfiles, findProfile } from "../profiles/index.js";

describe("profiles", () => {
  it("discoverProfiles returns an array", () => {
    const profiles = discoverProfiles();
    assert.ok(Array.isArray(profiles));
  });

  it("each profile has required fields", () => {
    const profiles = discoverProfiles();
    for (const p of profiles) {
      assert.ok(p.name, "profile should have name");
      assert.ok(p.browser, "profile should have browser");
      assert.ok(p.profileDir, "profile should have profileDir");
      assert.ok(p.cookiesPath, "profile should have cookiesPath");
    }
  });

  it("findProfile returns null for nonexistent browser", () => {
    const p = findProfile("nonexistent_browser_xyz");
    assert.strictEqual(p, null);
  });

  it("profile names follow browser/profile format", () => {
    const profiles = discoverProfiles();
    for (const p of profiles) {
      assert.match(p.name, /^\w+\/.+$/, `name should be browser/profile: ${p.name}`);
    }
  });
});
