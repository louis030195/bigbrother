import * as readline from "readline";
import type { BrowserSession } from "./launcher.js";
import { handleMethod } from "./handler.js";

/**
 * JSON-RPC server over stdin/stdout for programmatic browser control.
 *
 * Protocol: one JSON object per line (newline-delimited JSON).
 *
 * Request:  { "id": 1, "method": "navigate", "params": { "url": "..." } }
 * Response: { "id": 1, "result": { ... } }
 * Error:    { "id": 1, "error": { "message": "..." } }
 */
export async function startServer(session: BrowserSession): Promise<void> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    terminal: false,
  });

  const respond = (id: number | string, result: unknown) => {
    process.stdout.write(JSON.stringify({ id, result }) + "\n");
  };

  const respondError = (id: number | string, message: string) => {
    process.stdout.write(JSON.stringify({ id, error: { message } }) + "\n");
  };

  rl.on("line", async (line) => {
    let req: { id: number | string; method: string; params?: Record<string, unknown> };

    try {
      req = JSON.parse(line);
    } catch {
      process.stdout.write(
        JSON.stringify({ id: null, error: { message: "Invalid JSON" } }) + "\n"
      );
      return;
    }

    if (!req.method) {
      respondError(req.id ?? null, "Missing 'method' field");
      return;
    }

    try {
      const result = await handleMethod(session, req.method, req.params || {});
      respond(req.id, result);
      if (req.method === "close") process.exit(0);
    } catch (err) {
      respondError(req.id, err instanceof Error ? err.message : String(err));
    }
  });

  rl.on("close", async () => {
    await session.cleanup();
    process.exit(0);
  });

  // Keep alive
  await new Promise(() => {});
}
