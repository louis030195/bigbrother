// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
export interface Request {
  id: string | number | null;
  method: string;
  params: Record<string, unknown>;
}
export const maxRequestBytes = 1024 * 1024;
export function parseRequest(line: string): Request {
  if (Buffer.byteLength(line) > maxRequestBytes)
    throw new Error("Request exceeds 1 MiB");
  const value = JSON.parse(line);
  if (!value || Array.isArray(value) || typeof value !== "object")
    throw new Error("Request must be an object");
  if (typeof value.method !== "string" || !value.method)
    throw new Error("Missing method");
  if (
    value.id !== undefined &&
    value.id !== null &&
    typeof value.id !== "string" &&
    typeof value.id !== "number"
  )
    throw new Error("Invalid request id");
  if (
    value.params !== undefined &&
    (!value.params ||
      Array.isArray(value.params) ||
      typeof value.params !== "object")
  )
    throw new Error("params must be an object");
  return {
    id: value.id ?? null,
    method: value.method,
    params: value.params ?? {},
  };
}
export function validateSite(site: string): string {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$/.test(site))
    throw new Error(
      "Site must contain 1–64 letters, digits, underscores or hyphens",
    );
  return site;
}
