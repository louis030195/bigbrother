#!/usr/bin/env node

import { launch } from "./launcher.js";
import { startServer } from "./server.js";
import { handleMethod } from "./handler.js";
import { isSessionAlive, startSessionDaemon, sendToSession, socketPath } from "./session.js";
import { discoverProfiles, findProfile } from "./profiles/index.js";
import { extractCookies } from "./cookies.js";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";

// ── Helpers ──────────────────────────────────────────────────────────────

function out(data: unknown) {
  console.log(JSON.stringify(data, null, 2));
}

function die(msg: string, code = 1): never {
  console.error(`error: ${msg}`);
  process.exit(code);
}

function getFlag(args: string[], flag: string): string | undefined {
  const idx = args.indexOf(flag);
  if (idx === -1 || idx + 1 >= args.length) return undefined;
  return args[idx + 1];
}

function hasFlag(args: string[], flag: string): boolean {
  return args.includes(flag);
}

/**
 * Run a method against a site — uses session if alive, otherwise one-shot.
 */
async function run(
  site: string,
  method: string,
  params: Record<string, unknown>,
  opts: { url?: string; browser?: string } = {}
): Promise<unknown> {
  if (isSessionAlive(site)) {
    // Navigate first if URL provided and different from current
    if (opts.url) {
      await sendToSession(site, "navigate", { url: opts.url });
    }
    return sendToSession(site, method, params);
  }

  // One-shot: launch, optionally navigate, execute, close
  const session = await launch({
    browser: opts.browser || "chrome",
    headless: true,
    url: opts.url || undefined,
    site,
    persistent: true,
  });

  try {
    if (opts.url) {
      // Wait a bit for dynamic content
      await session.page.waitForTimeout(2000);
    }
    return await handleMethod(session, method, params);
  } finally {
    await session.cleanup();
  }
}

// ── Commands ─────────────────────────────────────────────────────────────

async function cmdOpen(site: string, args: string[]) {
  if (isSessionAlive(site)) {
    const url = getFlag(args, "--url");
    if (url) {
      const result = await sendToSession(site, "navigate", { url });
      out(result);
    } else {
      out({ session: site, socket: socketPath(site), status: "already_running" });
    }
    return;
  }

  const url = getFlag(args, "--url");
  const browser = getFlag(args, "--browser") || "chrome";
  const capture = hasFlag(args, "--capture");

  await startSessionDaemon(site, {
    browser,
    headless: true,
    url: url || undefined,
    site,
    persistent: true,
    captureNetwork: capture,
  } as any);
}

async function cmdClose(site: string) {
  if (!isSessionAlive(site)) {
    out({ session: site, status: "not_running" });
    return;
  }
  const result = await sendToSession(site, "close");
  out({ session: site, closed: true });
}

async function cmdText(site: string, args: string[]) {
  const url = getFlag(args, "--url");
  const result = await run(site, "text", {}, { url });
  out(result);
}

async function cmdSnap(site: string, args: string[]) {
  const url = getFlag(args, "--url");
  const result = await run(site, "snapshot", {}, { url });
  out(result);
}

async function cmdEval(site: string, expression: string, args: string[]) {
  const url = getFlag(args, "--url");
  const result = await run(site, "evaluate", { expression }, { url });
  out(result);
}

async function cmdShot(site: string, args: string[]) {
  const url = getFlag(args, "--url");
  const outPath = getFlag(args, "-o") || getFlag(args, "--output");
  const result = await run(site, "screenshot", {
    path: outPath || undefined,
    fullPage: hasFlag(args, "--full"),
  }, { url });
  out(result);
}

async function cmdClick(site: string, target: string, args: string[]) {
  const params: Record<string, unknown> = {};
  if (target.startsWith("#") || target.startsWith(".") || target.includes("[")) {
    params.selector = target;
  } else {
    params.text = target;
  }
  const result = await run(site, "click", params);
  out(result);
}

async function cmdType(site: string, text: string, args: string[]) {
  const selector = getFlag(args, "--selector") || getFlag(args, "-s");
  const params: Record<string, unknown> = { text };
  if (selector) params.selector = selector;
  params.humanlike = hasFlag(args, "--human");
  const result = await run(site, "type", params);
  out(result);
}

async function cmdPress(site: string, key: string) {
  const result = await run(site, "press", { key });
  out(result);
}

async function cmdScroll(site: string, args: string[]) {
  const direction = hasFlag(args, "--up") ? "up" : "down";
  const amount = parseInt(getFlag(args, "--amount") || "500", 10);
  const result = await run(site, "scroll", { direction, amount });
  out(result);
}

async function cmdNetwork(site: string, args: string[]) {
  if (!isSessionAlive(site)) {
    die(`no session running for "${site}". Start one with: bb web open ${site} --capture`);
  }
  const filter = getFlag(args, "--filter") || "";
  const method = getFlag(args, "--method") || "";
  const result = await sendToSession(site, "network", { filter, method });
  out(result);
}

async function cmdSeed(site: string, args: string[]) {
  const from = getFlag(args, "--from") || "chrome";
  const domain = getFlag(args, "--domain");
  if (!domain) die("--domain is required for seed");

  const profile = findProfile(from);
  if (!profile) die(`no ${from} profile found`);

  const cookies = extractCookies(profile.cookiesPath, profile.browser, domain);
  if (cookies.length === 0) {
    out({ seeded: 0, from, domain, note: "no cookies found for this domain" });
    return;
  }

  // Open the site profile, inject cookies, close
  const profileDir = path.join(os.homedir(), ".bb-browser-profiles", site);
  fs.mkdirSync(profileDir, { recursive: true });

  const { chromium } = await import("playwright");

  // Clean stale lock
  const lockFile = path.join(profileDir, "SingletonLock");
  if (fs.existsSync(lockFile)) fs.unlinkSync(lockFile);

  const context = await chromium.launchPersistentContext(profileDir, {
    headless: true,
    args: ["--disable-blink-features=AutomationControlled", "--no-first-run"],
  });

  let injected = 0;
  for (const cookie of cookies) {
    try {
      await context.addCookies([cookie]);
      injected++;
    } catch {}
  }

  await context.close();
  out({ seeded: injected, total: cookies.length, from, domain });
}

async function cmdLogin(site: string, args: string[]) {
  const url = getFlag(args, "--url");
  if (!url) die("--url is required for login");
  const browser = getFlag(args, "--browser") || "chrome";

  process.stderr.write(`opening ${url} for manual login...\n`);
  process.stderr.write("log in, then press Ctrl+C to save and exit.\n");

  const session = await launch({
    browser,
    headless: false,
    url,
    site,
    persistent: true,
  });

  process.on("SIGINT", async () => {
    process.stderr.write("\nsaving profile...\n");
    await session.cleanup();
    out({ site, url: session.page.url(), status: "saved" });
    process.exit(0);
  });

  await new Promise(() => {});
}

async function cmdScrape(site: string, args: string[]) {
  const url = args.find((a) => a.startsWith("http")) || getFlag(args, "--url");
  if (!url) die("URL required");
  const result = await run(site, "text", {}, { url });
  out(result);
}

async function cmdProfiles() {
  const profiles = discoverProfiles();
  out(profiles.length > 0 ? profiles : { profiles: [], note: "none found" });
}

async function cmdCookies(args: string[]) {
  const browser = getFlag(args, "--browser") || "chrome";
  const domain = getFlag(args, "--domain");
  const profile = findProfile(browser);
  if (!profile) die(`no ${browser} profile found`);

  const cookies = extractCookies(profile.cookiesPath, profile.browser, domain || undefined);
  out({
    profile: profile.name,
    count: cookies.length,
    cookies: domain ? cookies : cookies.slice(0, 20).map((c) => ({
      name: c.name, domain: c.domain, secure: c.secure,
    })),
    ...(domain ? {} : { note: "showing first 20. use --domain to filter." }),
  });
}

// Legacy: stdin/stdout server mode
async function cmdServe(args: string[]) {
  const site = args[0];
  const browser = getFlag(args, "--browser") || "chrome";
  const url = getFlag(args, "--url");

  const session = await launch({
    browser,
    headless: true,
    url: url || undefined,
    site: site || undefined,
    persistent: true,
  });

  process.stdout.write(
    JSON.stringify({
      ready: true,
      profile: site || "default",
      cookies: session.cookieStats,
      url: session.page.url(),
    }) + "\n"
  );

  await startServer(session);
}

// ── Main ─────────────────────────────────────────────────────────────────

function printUsage() {
  process.stderr.write(`bb-browser — browser automation with real auth

Usage: bb-browser <command> <site> [options]

Session:
  open <site>       Start persistent session (daemon)
                    --url <url>  --capture  --browser <name>
  close <site>      Stop session daemon

Read:
  text <site>       Get visible page text
  snap <site>       Aria snapshot (structured, for AI)
  shot <site>       Take screenshot (-o <file>  --full)
  eval <site> <js>  Evaluate JavaScript expression
  scrape <site> <url>  One-shot: navigate + text + close

Interact:
  click <site> <target>   Click element (CSS selector or text)
  type <site> <text>      Type text (--selector <s>  --human)
  press <site> <key>      Press key (Enter, Tab, etc.)
  scroll <site>           Scroll (--up  --amount <px>)

Auth:
  seed <site>       Seed cookies from real browser
                    --from <browser>  --domain <domain>
  login <site>      Open visible browser for manual login
                    --url <url>

Network:
  network <site>    Show captured API calls (needs --capture on open)
                    --filter <str>  --method <GET|POST|...>

Utility:
  profiles          List discovered browser profiles
  cookies           Show extracted cookies (--browser  --domain)
  serve [site]      Legacy stdin/stdout JSON-RPC mode

Common flags:  --url <url>  --browser <name>

Examples:
  bb-browser seed plaky --from arc --domain cake.com
  bb-browser scrape plaky https://app.plaky.com
  bb-browser open plaky --url https://app.plaky.com --capture
  bb-browser snap plaky
  bb-browser eval plaky "document.title"
  bb-browser network plaky --filter api.plaky
  bb-browser close plaky
`);
}

async function main() {
  const args = process.argv.slice(2);
  const command = args[0];

  if (!command || command === "--help" || command === "-h") {
    printUsage();
    process.exit(0);
  }

  const site = args[1];
  const rest = args.slice(2);

  switch (command) {
    // Session
    case "open":
      if (!site) die("site name required");
      return cmdOpen(site, rest);
    case "close":
      if (!site) die("site name required");
      return cmdClose(site);

    // Read
    case "text":
      if (!site) die("site name required");
      return cmdText(site, rest);
    case "snap":
      if (!site) die("site name required");
      return cmdSnap(site, rest);
    case "shot":
    case "screenshot":
      if (!site) die("site name required");
      return cmdShot(site, rest);
    case "eval":
      if (!site) die("site name required");
      if (!args[2]) die("JS expression required");
      // Expression is the next positional arg, rest are flags
      return cmdEval(site, args[2], args.slice(3));
    case "scrape":
      if (!site) die("site name required");
      return cmdScrape(site, rest);

    // Interact
    case "click":
      if (!site) die("site name required");
      if (!args[2]) die("target required (selector or text)");
      return cmdClick(site, args[2], rest);
    case "type":
      if (!site) die("site name required");
      if (!args[2]) die("text required");
      return cmdType(site, args[2], args.slice(3));
    case "press":
      if (!site) die("site name required");
      if (!args[2]) die("key required");
      return cmdPress(site, args[2]);
    case "scroll":
      if (!site) die("site name required");
      return cmdScroll(site, rest);

    // Auth
    case "seed":
      if (!site) die("site name required");
      return cmdSeed(site, rest);
    case "login":
      if (!site) die("site name required");
      return cmdLogin(site, rest);

    // Network
    case "network":
      if (!site) die("site name required");
      return cmdNetwork(site, rest);

    // Utility
    case "profiles":
      return cmdProfiles();
    case "cookies":
      return cmdCookies(args.slice(1));
    case "serve":
      return cmdServe(args.slice(1));

    default:
      // Maybe the user passed a URL as first arg (shortcut for scrape)
      if (command.startsWith("http")) {
        die(`did you mean: bb-browser scrape <site> ${command}`);
      }
      die(`unknown command: ${command}. Run bb-browser --help`);
  }
}

main().catch((err) => {
  const msg = err instanceof Error ? err.message : String(err);

  if (msg.includes("profile found")) {
    process.stderr.write(`error: ${msg}\navailable profiles:\n`);
    for (const p of discoverProfiles()) {
      process.stderr.write(`  ${p.name} (${p.profileDir})\n`);
    }
    process.exit(2);
  }

  if (msg.includes("ProcessSingleton") || msg.includes("SingletonLock")) {
    die("browser profile is locked (another instance may be running). try: bb-browser close <site>", 3);
  }

  if (msg.includes("Executable doesn't exist")) {
    die("chrome not found. install Google Chrome or run: npx playwright install chromium", 3);
  }

  if (msg.includes("Keychain") || msg.includes("security find-generic-password")) {
    die("could not access keychain to decrypt cookies", 4);
  }

  if (msg.includes("Session connection failed")) {
    die(`${msg}. try: bb-browser close <site> && bb-browser open <site>`, 5);
  }

  die(msg);
});
