import { chromium, type Browser, type BrowserContext, type Page } from "playwright";
import * as os from "os";
import * as path from "path";
import * as fs from "fs";
import { extractCookies } from "./cookies.js";
import { findProfile, type BrowserProfile } from "./profiles/index.js";

export interface LaunchOptions {
  /** Browser to steal cookies from: chrome, arc, brave, edge */
  browser: string;
  /** Run headless (background mode) */
  headless: boolean;
  /** Initial URL to navigate to */
  url?: string;
  /** Only extract cookies for these domains */
  domains?: string[];
  /** Custom user data dir (skip profile discovery) */
  userDataDir?: string;
  /** Use persistent context for reliable auth (like real browser sessions) */
  persistent?: boolean;
  /** Site name for per-site profile isolation (e.g. "linkedin", "reddit") */
  site?: string;
  /** Enable network request capture (used by session daemon) */
  captureNetwork?: boolean;
}

export interface CookieStats {
  total: number;
  injected: number;
  skipped: number;
  errors: string[];
}

export interface BrowserSession {
  browser: Browser | null;
  context: BrowserContext;
  page: Page;
  profile: BrowserProfile | null;
  cookieStats: CookieStats;
  cleanup: () => Promise<void>;
}

const PERSISTENT_PROFILE_DIR = path.join(os.homedir(), ".bb-browser-profile");

/**
 * Launch a browser session.
 *
 * Two modes:
 * 1. **Persistent** (default): uses `launchPersistentContext` with a stable profile dir.
 *    First run requires manual login; subsequent runs reuse the full session
 *    (cookies, localStorage, IndexedDB, service workers). This is how real browsers work
 *    and is far more reliable for sites with aggressive bot detection (LinkedIn, etc).
 *
 * 2. **Cookie injection** (legacy, `persistent: false`): launches a fresh context and
 *    injects cookies extracted from another browser. Fragile — many sites detect this.
 */
export async function launch(options: LaunchOptions): Promise<BrowserSession> {
  const usePersistent = options.persistent !== false;

  if (usePersistent) {
    return launchPersistent(options);
  }
  return launchWithCookies(options);
}

async function launchPersistent(options: LaunchOptions): Promise<BrowserSession> {
  // Per-site profile isolation: ~/.bb-browser-profiles/linkedin/, etc.
  const profileDir = options.userDataDir
    || (options.site
      ? path.join(os.homedir(), ".bb-browser-profiles", options.site)
      : PERSISTENT_PROFILE_DIR);
  fs.mkdirSync(profileDir, { recursive: true });

  // Clean stale lock file from crashed sessions
  const lockFile = path.join(profileDir, "SingletonLock");
  if (fs.existsSync(lockFile)) {
    fs.unlinkSync(lockFile);
  }

  // Use system Chrome if available (better stealth), fall back to Playwright Chromium
  let channel: "chrome" | undefined = "chrome";
  try {
    // Quick check: can we find Chrome?
    const testBrowser = await chromium.launch({ channel: "chrome", headless: true });
    await testBrowser.close();
  } catch {
    channel = undefined; // Chrome not available, use bundled Chromium
  }

  const context = await chromium.launchPersistentContext(profileDir, {
    headless: options.headless,
    ...(channel ? { channel } : {}),
    args: [
      "--disable-blink-features=AutomationControlled",
      "--no-first-run",
      "--no-default-browser-check",
    ],
    viewport: { width: 1440, height: 900 },
    locale: Intl.DateTimeFormat().resolvedOptions().locale,
    timezoneId: Intl.DateTimeFormat().resolvedOptions().timeZone,
  });

  // Remove webdriver flag
  await context.addInitScript(() => {
    Object.defineProperty(navigator, "webdriver", {
      get: () => false,
    });
  });

  // Seed cookies from real browser into persistent profile
  // This makes first launch work without manual login
  const cookieStats: CookieStats = { total: 0, injected: 0, skipped: 0, errors: [] };
  const profile = findProfile(options.browser);

  if (profile) {
    const domains = options.domains;
    let cookies: Awaited<ReturnType<typeof extractCookies>>;
    try {
      cookies = extractCookies(
        profile.cookiesPath,
        profile.browser,
        domains ? domains[0] : undefined
      );
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      cookieStats.errors.push(`Cookie extraction failed: ${msg}`);
      process.stderr.write(`warning: cookie extraction failed: ${msg}\n`);
      cookies = [];
    }

    cookieStats.total = cookies.length;

    if (cookies.length > 0) {
      const batchSize = 50;
      for (let i = 0; i < cookies.length; i += batchSize) {
        const batch = cookies.slice(i, i + batchSize);
        try {
          await context.addCookies(batch);
          cookieStats.injected += batch.length;
        } catch {
          for (const cookie of batch) {
            try {
              await context.addCookies([cookie]);
              cookieStats.injected++;
            } catch (err) {
              cookieStats.skipped++;
              if (cookieStats.errors.length < 5) {
                cookieStats.errors.push(
                  `${cookie.domain}/${cookie.name}: ${err instanceof Error ? err.message : String(err)}`
                );
              }
            }
          }
        }
      }
    }

    if (cookieStats.injected > 0) {
      process.stderr.write(
        `seeded ${cookieStats.injected}/${cookieStats.total} cookies from ${options.browser}\n`
      );
    }
  }

  const page = context.pages()[0] ?? (await context.newPage());

  if (options.url) {
    await page.goto(options.url, { waitUntil: "domcontentloaded" });
  }

  return {
    browser: null,
    context,
    page,
    profile,
    cookieStats,
    cleanup: async () => {
      await context.close();
    },
  };
}

async function launchWithCookies(options: LaunchOptions): Promise<BrowserSession> {
  const profile = options.userDataDir
    ? null
    : findProfile(options.browser);

  // No profile is OK — just launch without cookies
  if (!profile && !options.userDataDir) {
    process.stderr.write(
      `warning: no ${options.browser} profile found, launching without cookies\n`
    );
  }

  // Create isolated user data dir so we don't conflict with the running browser
  const tmpDir = fs.mkdtempSync(
    path.join(os.tmpdir(), "bb-browser-")
  );

  // Use system Chrome if available (better stealth), fall back to Playwright Chromium
  let useChannel: "chrome" | undefined = "chrome";
  try {
    const testBrowser = await chromium.launch({ channel: "chrome", headless: true });
    await testBrowser.close();
  } catch {
    useChannel = undefined;
  }

  const browser = await chromium.launch({
    ...(useChannel ? { channel: useChannel } : {}),
    headless: options.headless,
    args: [
      "--disable-blink-features=AutomationControlled",
      "--no-first-run",
      "--no-default-browser-check",
    ],
  });

  const context = await browser.newContext({
    userAgent: getRealisticUserAgent(),
    viewport: { width: 1440, height: 900 },
    locale: Intl.DateTimeFormat().resolvedOptions().locale,
    timezoneId: Intl.DateTimeFormat().resolvedOptions().timeZone,
  });

  // Inject cookies from real browser
  const cookieStats: CookieStats = { total: 0, injected: 0, skipped: 0, errors: [] };

  if (profile) {
    const domains = options.domains;
    let cookies: Awaited<ReturnType<typeof extractCookies>>;
    try {
      cookies = extractCookies(
        profile.cookiesPath,
        profile.browser,
        domains ? domains[0] : undefined
      );
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      cookieStats.errors.push(`Cookie extraction failed: ${msg}`);
      process.stderr.write(`warning: cookie extraction failed: ${msg}\n`);
      cookies = [];
    }

    cookieStats.total = cookies.length;

    if (cookies.length > 0) {
      const batchSize = 50;
      for (let i = 0; i < cookies.length; i += batchSize) {
        const batch = cookies.slice(i, i + batchSize);
        try {
          await context.addCookies(batch);
          cookieStats.injected += batch.length;
        } catch {
          for (const cookie of batch) {
            try {
              await context.addCookies([cookie]);
              cookieStats.injected++;
            } catch (err) {
              cookieStats.skipped++;
              if (cookieStats.errors.length < 5) {
                cookieStats.errors.push(
                  `${cookie.domain}/${cookie.name}: ${err instanceof Error ? err.message : String(err)}`
                );
              }
            }
          }
        }
      }
    }

    if (cookieStats.skipped > 0) {
      process.stderr.write(
        `warning: injected ${cookieStats.injected}/${cookieStats.total} cookies, ${cookieStats.skipped} skipped\n`
      );
    }
  }

  // Remove webdriver flag
  await context.addInitScript(() => {
    Object.defineProperty(navigator, "webdriver", {
      get: () => false,
    });
  });

  const page = await context.newPage();

  if (options.url) {
    await page.goto(options.url, { waitUntil: "domcontentloaded" });
  }

  return {
    browser,
    context,
    page,
    profile,
    cookieStats,
    cleanup: async () => {
      await context.close();
      await browser.close();
      try {
        fs.rmSync(tmpDir, { recursive: true, force: true });
      } catch {}
    },
  };
}

function getRealisticUserAgent(): string {
  const platform = os.platform();
  const arch = os.arch();

  if (platform === "darwin") {
    const macVersion = os.release().split(".")[0];
    const macOSVersion = parseInt(macVersion) >= 24 ? "15_0" : "14_0";
    return `Mozilla/5.0 (Macintosh; Intel Mac OS X ${macOSVersion}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36`;
  }

  if (platform === "win32") {
    return "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
  }

  return "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
}
