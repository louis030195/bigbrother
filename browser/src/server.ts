// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
import type { BrowserSession } from "./launcher.js";
import { handleMethod } from "./handler.js";
import { maxRequestBytes, parseRequest } from "./protocol.js";

/** Bounded newline-delimited JSON. EOF drains queued work before closing. */
export async function startServer(session: BrowserSession): Promise<void> {
  process.stdin.setEncoding("utf8");
  let buffer = "", closing = false;
  try {
    for await (const chunk of process.stdin) {
      buffer += chunk;
      while (buffer.includes("\n")) {
        const index = buffer.indexOf("\n"), line = buffer.slice(0, index); buffer = buffer.slice(index + 1);
        if (!line.trim()) continue;
        let id: string | number | null = null;
        try {
          const req = parseRequest(line); id = req.id;
          const result = await handleMethod(session, req.method, req.params);
          process.stdout.write(JSON.stringify({ id, result }) + "\n");
          if (req.method === "close") { closing = true; return; }
        } catch (error) { process.stdout.write(JSON.stringify({ id, error: { message: String(error instanceof Error ? error.message : error) } }) + "\n"); }
      }
      if (Buffer.byteLength(buffer) > maxRequestBytes) throw new Error("Request exceeds 1 MiB");
    }
    if (buffer.trim()) process.stdout.write(JSON.stringify({ id: null, error: { message: "Unterminated request at EOF" } }) + "\n");
  } finally { if (!closing) await session.cleanup(); }
}
