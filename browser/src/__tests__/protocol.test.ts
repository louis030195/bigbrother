// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
import { test, expect } from "bun:test";
import { parseRequest, validateSite, maxRequestBytes } from "../protocol.js";
test("protocol validates shape and size without accepting arrays/null", () => {
  for (const bad of [
    "null",
    "[]",
    '{"method":1}',
    '{"method":"observe","params":[]}',
    '{"method":"observe","id":{}}',
  ])
    expect(() => parseRequest(bad)).toThrow();
  expect(parseRequest('{"id":0,"method":"observe"}')).toEqual({
    id: 0,
    method: "observe",
    params: {},
  });
  expect(() => parseRequest(" ".repeat(maxRequestBytes + 1))).toThrow();
});
test("session names cannot escape profile/socket directories", () => {
  for (const bad of [
    "../human",
    "/tmp/human",
    "",
    "a/b",
    "a\\b",
    "a".repeat(65),
  ])
    expect(() => validateSite(bad)).toThrow();
  expect(validateSite("task_123-test")).toBe("task_123-test");
});

test("stdio handles pipelined requests in order and drains before EOF", async () => {
  const child = Bun.spawn(
    [
      process.execPath,
      require("node:path").join(__dirname, "fixtures/stdio.ts"),
    ],
    { stdin: "pipe", stdout: "pipe", stderr: "pipe", env: process.env },
  );
  child.stdin.write(
    'null\n{"id":1,"method":"observe"}\n{"id":2,"method":"observe"}\n',
  );
  child.stdin.end();
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  expect(code).toBe(0);
  expect(stderr).toBe("");
  const replies = stdout
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line));
  expect(replies[0].error).toBeDefined();
  expect(replies[1].id).toBe(1);
  expect(replies[2].id).toBe(2);
  expect(replies[1].result.revision).toBe(1);
  expect(replies[2].result.mode).toBe("unchanged");
}, 15000);

test("Windows sessions use a valid named-pipe path", async () => {
  const { socketPath } = await import("../session.js");
  expect(socketPath("fixture", "win32").startsWith("\\\\.\\pipe\\bb-")).toBe(
    true,
  );
});
