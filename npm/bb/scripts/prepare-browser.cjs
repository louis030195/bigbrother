// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
const fs = require("node:fs");
const path = require("node:path");
const source = path.resolve(__dirname, "../../../browser/src");
const target = path.resolve(__dirname, "../browser");
if (!fs.existsSync(source))
  throw new Error(
    "Package from a repository checkout: browser/src is required",
  );
fs.rmSync(target, { recursive: true, force: true });
fs.cpSync(source, target, {
  recursive: true,
  filter: (file) => !file.includes(`${path.sep}__tests__`),
});
