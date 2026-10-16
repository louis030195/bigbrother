// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
import { test, expect } from "bun:test";
import {
  Renderer,
  defaults,
  clip,
  limitsFor,
  type Node,
} from "../observation.js";
const node = (key: string, name = "Save"): Node => ({
  key,
  role: "button",
  name,
  value: "",
  states: [],
  actions: ["click"],
});
const capture = (nodes: Node[]) => ({
  target: "fixture",
  source: "fixture",
  nodes,
  warnings: [],
});
test("stable refs, Unicode, protected values and strict budgets", () => {
  const r = new Renderer();
  expect(clip("🙂你好a", 3)).toBe("🙂你好…");
  const secret = {
    ...node("secret", "Password"),
    protected: true,
    value: "never-print-this",
  };
  expect(r.render(capture([secret]), defaults).text).not.toContain(
    "never-print-this",
  );
  const result = r.render(
    capture(
      Array.from({ length: 100 }, (_, i) => node(`${i}`, "汉".repeat(200))),
    ),
    { ...defaults, maxOutputBytes: 512 },
  );
  expect(result.truncated).toBe(true);
  expect(Buffer.byteLength(result.text)).toBeLessThanOrEqual(512);
  expect(() => limitsFor({ maxNodes: -1 })).toThrow();
});
test("diffs are cheaper than full output; structural changes return full", () => {
  const r = new Renderer(),
    nodes = Array.from({ length: 100 }, (_, i) => node(`${i}`));
  const full = r.render(capture(nodes), defaults);
  const unchanged = r.render(capture(nodes), defaults);
  expect(unchanged.mode).toBe("unchanged");
  expect(() => r.resolve(full.revision, "e1")).toThrow("STALE_OBSERVATION");
  const changed = [...nodes];
  changed[0] = node("0", "Saved");
  const diff = r.render(capture(changed), defaults);
  expect(diff.mode).toBe("diff");
  expect(diff.base_revision).toBe(2);
  expect(diff.text.length).toBeLessThan(full.text.length / 10);
  const moved = r.render(capture([...changed].reverse()), defaults);
  expect(moved.mode).toBe("full");
  r.invalidate();
  expect(() => r.resolve(moved.revision, "e1")).toThrow();
});
test("empty wrappers flatten without orphaning children or recycling references", () => {
  const r = new Renderer();
  const out = r.render(
    capture([
      { ...node("group", ""), role: "group", actions: [] },
      { ...node("child"), parent: "group" },
    ]),
    defaults,
  );
  expect(out.node_count).toBe(1);
  expect(out.text).not.toContain("parent=");
  const next = r.render(capture([node("different")]), defaults);
  expect(next.text).toContain("[e2]");
  expect(() => r.resolve(next.revision, "e1")).toThrow();
});
