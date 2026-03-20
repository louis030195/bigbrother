import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as crypto from "crypto";
import { execSync } from "child_process";
import { Database } from "bun:sqlite";
import type { Cookie } from "playwright";

/**
 * Extract and decrypt cookies from a Chromium browser profile.
 *
 * Chromium encrypts cookies differently per platform:
 * - macOS: AES-128-CBC, key from Keychain ("Chrome Safe Storage")
 * - Linux: AES-128-CBC, key from libsecret or hardcoded "peanuts"
 * - Windows: DPAPI (CryptUnprotectData)
 */
export function extractCookies(
  cookiesDbPath: string,
  browser: string,
  domain?: string
): Cookie[] {
  // Copy DB to temp file (Chrome locks the original)
  const tmpDb = path.join(
    os.tmpdir(),
    `bb-cookies-${Date.now()}-${Math.random().toString(36).slice(2)}.db`
  );
  fs.copyFileSync(cookiesDbPath, tmpDb);

  // Also copy WAL/SHM files if they exist (for consistency)
  for (const suffix of ["-wal", "-shm"]) {
    const src = cookiesDbPath + suffix;
    if (fs.existsSync(src)) {
      fs.copyFileSync(src, tmpDb + suffix);
    }
  }

  try {
    const db = new Database(tmpDb, { readonly: true });
    const key = getDecryptionKey(browser);

    let query =
      "SELECT host_key, name, path, encrypted_value, is_secure, is_httponly, samesite, expires_utc FROM cookies";
    const params: string[] = [];

    if (domain) {
      query += " WHERE host_key LIKE ?";
      params.push(`%${domain}%`);
    }

    const stmt = db.prepare(query);
    const rows = (params.length > 0 ? stmt.all(params[0]) : stmt.all()) as CookieRow[];
    const cookies: Cookie[] = [];

    for (const row of rows) {
      const value = decryptValue(row.encrypted_value, key);
      if (value === null) continue;

      // Skip cookies with empty name or value, or invalid domain
      if (!row.name || !row.host_key) continue;

      // Playwright requires domain to start with "." for domain cookies,
      // or match exactly. Filter out malformed entries.
      const domain = row.host_key;
      const expires = row.expires_utc
        ? chromiumTimestampToUnix(row.expires_utc)
        : -1;

      // Skip expired cookies (negative unix timestamp that isn't -1 session cookie)
      if (expires !== -1 && expires < Date.now() / 1000) continue;

      cookies.push({
        name: row.name,
        value,
        domain,
        path: row.path || "/",
        secure: row.is_secure === 1,
        httpOnly: row.is_httponly === 1,
        sameSite: parseSameSite(row.samesite),
        expires,
      });
    }

    db.close();
    return cookies;
  } finally {
    // Clean up temp files
    for (const f of [tmpDb, tmpDb + "-wal", tmpDb + "-shm"]) {
      try {
        fs.unlinkSync(f);
      } catch {}
    }
  }
}

interface CookieRow {
  host_key: string;
  name: string;
  path: string;
  encrypted_value: Uint8Array | Buffer;
  is_secure: number;
  is_httponly: number;
  samesite: number;
  expires_utc: number;
}

function parseSameSite(value: number): "Strict" | "Lax" | "None" {
  switch (value) {
    case 2:
      return "Strict";
    case 1:
      return "Lax";
    default:
      return "None";
  }
}

// Chromium epoch: 1601-01-01 00:00:00 UTC
// Difference from Unix epoch in microseconds
const CHROMIUM_EPOCH_OFFSET = 11644473600n;

function chromiumTimestampToUnix(microseconds: number): number {
  if (microseconds === 0) return -1;
  const seconds = BigInt(microseconds) / 1000000n - CHROMIUM_EPOCH_OFFSET;
  return Number(seconds);
}

/**
 * Get the decryption key for Chromium cookies.
 */
function getDecryptionKey(browser: string): Buffer {
  const platform = os.platform();

  if (platform === "darwin") {
    return getMacOSKey(browser);
  } else if (platform === "linux") {
    return getLinuxKey();
  } else if (platform === "win32") {
    // Windows uses DPAPI — decryption is handled per-value in decryptValue
    return Buffer.alloc(0);
  }

  throw new Error(`Unsupported platform: ${platform}`);
}

function getMacOSKey(browser: string): Buffer {
  const keychainService = getKeychainService(browser);
  const password = execSync(
    `security find-generic-password -s "${keychainService}" -w`,
    { encoding: "utf8" }
  ).trim();

  // Derive key using PBKDF2 (same params Chrome uses)
  return crypto.pbkdf2Sync(password, "saltysalt", 1003, 16, "sha1");
}

function getKeychainService(browser: string): string {
  switch (browser.toLowerCase()) {
    case "chrome":
      return "Chrome Safe Storage";
    case "chromium":
      return "Chromium Safe Storage";
    case "brave":
      return "Brave Safe Storage";
    case "edge":
      return "Microsoft Edge Safe Storage";
    case "arc":
      return "Arc Safe Storage";
    default:
      return "Chrome Safe Storage";
  }
}

function getLinuxKey(): Buffer {
  // On Linux, Chrome uses libsecret or falls back to "peanuts"
  // For simplicity, try "peanuts" first (common for headless / no keyring)
  const password = "peanuts";
  return crypto.pbkdf2Sync(password, "saltysalt", 1, 16, "sha1");
}

/**
 * Decrypt a single cookie value.
 */
function decryptValue(encrypted: Uint8Array | Buffer, key: Buffer): string | null {
  if (!encrypted || encrypted.length === 0) return "";
  // Ensure we have a Buffer for crypto operations
  const buf = Buffer.isBuffer(encrypted) ? encrypted : Buffer.from(encrypted);

  const platform = os.platform();

  // macOS/Linux: v10 or v11 prefix = AES-128-CBC
  if (
    (platform === "darwin" || platform === "linux") &&
    buf.length > 3
  ) {
    const version = buf.subarray(0, 3).toString("utf8");
    if (version === "v10" || version === "v11") {
      try {
        const ciphertext = buf.subarray(3);
        const iv = Buffer.alloc(16, " "); // 16 spaces
        const decipher = crypto.createDecipheriv("aes-128-cbc", key, iv);
        let decrypted = decipher.update(ciphertext);
        decrypted = Buffer.concat([decrypted, decipher.final()]);
        // Modern Chromium prepends a 32-byte HMAC-SHA256 tag to the plaintext.
        // Strip it to get the actual cookie value.
        if (decrypted.length > 32) {
          return decrypted.subarray(32).toString("utf8");
        }
        return decrypted.toString("utf8");
      } catch {
        return null;
      }
    }
  }

  // Windows: DPAPI decryption via PowerShell
  if (platform === "win32" && buf.length > 0) {
    try {
      const b64 = buf.toString("base64");
      const result = execSync(
        `powershell -NoProfile -Command "[System.Text.Encoding]::UTF8.GetString([System.Security.Cryptography.ProtectedData]::Unprotect([System.Convert]::FromBase64String('${b64}'), $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser))"`,
        { encoding: "utf8" }
      ).trim();
      return result;
    } catch {
      return null;
    }
  }

  // Unencrypted value
  return buf.toString("utf8");
}
