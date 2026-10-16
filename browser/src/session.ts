// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
import { parseRequest, validateSite, maxRequestBytes } from "./protocol.js";
import { runtimeFor } from "./runtime.js";
import * as net from "net";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { launch, type LaunchOptions, type BrowserSession } from "./launcher.js";
import { handleMethod } from "./handler.js";

const SOCKET_DIR = path.join(os.tmpdir(), `bb-sessions-${process.getuid?.() ?? os.userInfo().username}`);

export function socketPath(site: string, platform: NodeJS.Platform = process.platform): string {
  validateSite(site);
  return platform === "win32" ? `\\\\.\\pipe\\bb-${os.userInfo().username}-${site}` : path.join(SOCKET_DIR, `bb-${site}.sock`);
}

function pidPath(site: string): string {
  validateSite(site);
  return path.join(SOCKET_DIR, `bb-${site}.pid`);
}

/**
 * Check if a session daemon is alive for the given site.
 * Require a live recorded PID and the transport endpoint. Connection errors
 * still surface to the caller; an endpoint alone does not prove liveness.
 */
export function isSessionAlive(site: string): boolean {
  try {
    const pid = Number(fs.readFileSync(pidPath(site), "utf8"));
    if (!Number.isInteger(pid) || pid <= 0) return false;
    process.kill(pid, 0);
    return process.platform === "win32" || fs.existsSync(socketPath(site));
  } catch { return false; }
}

/**
 * Start a session daemon that listens on a Unix socket.
 * This function blocks — it IS the daemon process.
 */
export async function startSessionDaemon(
  site: string,
  launchOpts: LaunchOptions
): Promise<void> {
  const sock = socketPath(site);
  const pid = pidPath(site);

  fs.mkdirSync(SOCKET_DIR, { recursive: true, mode: 0o700 });
  if (isSessionAlive(site)) throw new Error(`Session ${site} is already running`);
  if (process.platform !== "win32") { try { fs.unlinkSync(sock); } catch {} }

  const session = await launch(launchOpts);

  // Network capture buffer (enabled by caller via launchOpts)
  const networkLog: NetworkEntry[] = [];
  if ((launchOpts as any).captureNetwork) {
    session.page.on("request", (req) => {
      const url = req.url();
      if (url.startsWith("data:") || url.endsWith(".js") || url.endsWith(".css") ||
          url.endsWith(".png") || url.endsWith(".jpg") || url.endsWith(".svg") ||
          url.endsWith(".woff2") || url.endsWith(".woff")) return;
      if (networkLog.length >= 1000) networkLog.shift();
      networkLog.push({
        method: req.method(),
        url,
        postData: req.postData() || undefined,
        timestamp: Date.now(),
      });
    });
    session.page.on("response", async (res) => {
      const entry = networkLog.find(
        (e) => e.url === res.url() && !e.status
      );
      if (entry) {
        entry.status = res.status();
        try {
          const ct = res.headers()["content-type"] || "";
          if (ct.includes("json") || ct.includes("text")) {
            entry.body = (await res.text()).slice(0, 2000);
          }
        } catch {}
      }
    });
  }

  const server = net.createServer((conn) => {
    let buffer = "";
    conn.setEncoding("utf8");

    conn.on("data", (chunk) => {
      buffer += chunk.toString();
      if (Buffer.byteLength(buffer) > maxRequestBytes) {
        conn.end(JSON.stringify({ id: null, error: { message: "Request exceeds 1 MiB" } }) + "\n"); return;
      }
      const lines = buffer.split("\n");
      buffer = lines.pop() || "";

      for (const line of lines) {
        if (!line.trim()) continue;
        handleRequest(session, networkLog, line.trim()).then((response) => {
          conn.write(response + "\n");
          conn.end();
        });
      }
    });

    conn.on("error", () => {}); // ignore client disconnect
  });

  server.on("error", async (error) => { await session.cleanup(); process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
  server.listen(sock, () => {
    fs.writeFileSync(pid, String(process.pid));
    // Signal readiness to CLI
    const info = {
      session: site,
      socket: sock,
      pid: process.pid,
      url: session.page.url(),
    };
    process.stdout.write(JSON.stringify(info) + "\n");
  });

  const cleanup = async () => {
    try { server.close(); } catch {}
    try { fs.unlinkSync(sock); } catch {}
    try { fs.unlinkSync(pid); } catch {}
    try { await session.cleanup(); } catch {}
  };

  process.on("SIGINT", async () => { await cleanup(); process.exit(0); });
  process.on("SIGTERM", async () => { await cleanup(); process.exit(0); });
  process.on("uncaughtException", async () => { await cleanup(); process.exit(1); });

  // Keep alive
  await new Promise(() => {});
}

async function handleRequest(
  session: BrowserSession,
  networkLog: NetworkEntry[],
  line: string
): Promise<string> {
  let req: ReturnType<typeof parseRequest>;

  try {
    req = parseRequest(line);
  } catch {
    return JSON.stringify({ id: null, error: { message: "Invalid JSON" } });
  }

  // Special method: network log
  if (req.method === "network") {
    const filter = (req.params?.filter as string) || "";
    const methodFilter = (req.params?.method as string) || "";
    let entries = networkLog;
    if (filter) entries = entries.filter((e) => e.url.includes(filter));
    if (methodFilter) entries = entries.filter((e) => e.method === methodFilter.toUpperCase());
    return JSON.stringify({ id: req.id, result: { count: entries.length, calls: entries.slice(-100) } });
  }

  // Special method: close — shut down daemon
  if (req.method === "close") {
    await runtimeFor(session.page).run(() => session.cleanup());
    const response = JSON.stringify({ id: req.id, result: { closed: true } });
    // Schedule shutdown after responding — use SIGTERM to trigger cleanup handler
    setTimeout(() => {
      process.kill(process.pid, "SIGTERM");
    }, 100);
    return response;
  }

  try {
    const result = await handleMethod(session, req.method, req.params || {});
    return JSON.stringify({ id: req.id, result });
  } catch (err) {
    return JSON.stringify({
      id: req.id,
      error: { message: err instanceof Error ? err.message : String(err) },
    });
  }
}

/**
 * Send a command to a running session daemon via Unix socket.
 */
export function sendToSession(
  site: string,
  method: string,
  params: Record<string, unknown> = {}
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const sock = socketPath(site);
    const conn = net.createConnection(sock);
    conn.setEncoding("utf8");
    let data = "";

    conn.on("connect", () => {
      conn.write(JSON.stringify({ id: 1, method, params }) + "\n");
    });

    conn.on("data", (chunk) => {
      data += chunk.toString();
    });

    conn.on("end", () => {
      try {
        const parsed = JSON.parse(data.trim());
        if (parsed.error) {
          reject(new Error(parsed.error.message));
        } else {
          resolve(parsed.result);
        }
      } catch (err) {
        reject(new Error(`Invalid response from session: ${data}`));
      }
    });

    conn.on("error", (err) => {
      reject(new Error(`Session connection failed: ${err.message}`));
    });

    // Timeout after 30s
    conn.setTimeout(30000, () => {
      conn.destroy();
      reject(new Error("Session command timed out"));
    });
  });
}

interface NetworkEntry {
  method: string;
  url: string;
  postData?: string;
  timestamp: number;
  status?: number;
  body?: string;
}
