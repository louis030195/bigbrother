import * as path from "path";
import * as os from "os";
import * as fs from "fs";

export interface BrowserProfile {
  name: string;
  browser: string;
  profileDir: string;
  cookiesPath: string;
}

interface ProfileLocation {
  browser: string;
  /** Path segments relative to home dir per platform */
  darwin: string[];
  win32: string[];
  linux: string[];
}

const CHROMIUM_PROFILES: ProfileLocation[] = [
  {
    browser: "chrome",
    darwin: ["Library", "Application Support", "Google", "Chrome"],
    win32: ["AppData", "Local", "Google", "Chrome", "User Data"],
    linux: [".config", "google-chrome"],
  },
  {
    browser: "arc",
    darwin: ["Library", "Application Support", "Arc", "User Data"],
    win32: ["AppData", "Local", "Arc", "User Data"],
    linux: [],
  },
  {
    browser: "brave",
    darwin: [
      "Library",
      "Application Support",
      "BraveSoftware",
      "Brave-Browser",
    ],
    win32: [
      "AppData",
      "Local",
      "BraveSoftware",
      "Brave-Browser",
      "User Data",
    ],
    linux: [".config", "BraveSoftware", "Brave-Browser"],
  },
  {
    browser: "edge",
    darwin: ["Library", "Application Support", "Microsoft Edge"],
    win32: ["AppData", "Local", "Microsoft", "Edge", "User Data"],
    linux: [".config", "microsoft-edge"],
  },
  {
    browser: "chromium",
    darwin: ["Library", "Application Support", "Chromium"],
    win32: ["AppData", "Local", "Chromium", "User Data"],
    linux: [".config", "chromium"],
  },
];

function getPlatformKey(): "darwin" | "win32" | "linux" {
  const p = os.platform();
  if (p === "darwin" || p === "win32" || p === "linux") return p;
  throw new Error(`Unsupported platform: ${p}`);
}

/**
 * Find all Chromium-based browser profiles on this machine.
 */
export function discoverProfiles(): BrowserProfile[] {
  const platform = getPlatformKey();
  const home = os.homedir();
  const profiles: BrowserProfile[] = [];

  for (const loc of CHROMIUM_PROFILES) {
    const segments = loc[platform];
    if (!segments || segments.length === 0) continue;

    const baseDir = path.join(home, ...segments);
    if (!fs.existsSync(baseDir)) continue;

    // Chromium stores profiles in subdirectories: Default, Profile 1, Profile 2, etc.
    const profileDirs = fs
      .readdirSync(baseDir)
      .filter(
        (d) =>
          (d === "Default" || d.startsWith("Profile ")) &&
          fs.statSync(path.join(baseDir, d)).isDirectory()
      );

    for (const profileDir of profileDirs) {
      const fullDir = path.join(baseDir, profileDir);
      const cookiesPath = path.join(fullDir, "Cookies");

      if (fs.existsSync(cookiesPath)) {
        profiles.push({
          name: `${loc.browser}/${profileDir}`,
          browser: loc.browser,
          profileDir: fullDir,
          cookiesPath,
        });
      }
    }
  }

  return profiles;
}

/**
 * Find a specific browser profile by browser name.
 * Returns the Default profile, or the first available one.
 */
export function findProfile(browser: string): BrowserProfile | null {
  const profiles = discoverProfiles().filter(
    (p) => p.browser === browser.toLowerCase()
  );
  return (
    profiles.find((p) => p.name.endsWith("/Default")) || profiles[0] || null
  );
}
