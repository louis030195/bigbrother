// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
export interface Limits {
  maxNodes: number;
  maxDepth: number;
  maxChildren: number;
  maxTextChars: number;
  maxOutputBytes: number;
}
export const defaults: Limits = {
  maxNodes: 2000,
  maxDepth: 32,
  maxChildren: 200,
  maxTextChars: 500,
  maxOutputBytes: 32000,
};
export function limitsFor(input: Partial<Limits> = {}): Limits {
  const limits = { ...defaults, ...input };
  for (const [key, value] of Object.entries(limits)) {
    const cap = {
      maxNodes: 10000,
      maxDepth: 128,
      maxChildren: 1000,
      maxTextChars: 10000,
      maxOutputBytes: 1000000,
    }[key]!;
    if (
      !Number.isInteger(value) ||
      value < (key === "maxOutputBytes" ? 256 : 1) ||
      value > cap
    )
      throw new Error(`Invalid observation limit: ${key}`);
  }
  return limits;
}
export interface Node {
  key: string;
  parent?: string;
  role: string;
  name: string;
  value: string;
  states: string[];
  actions: string[];
  protected?: boolean;
  bounds?: number[];
}
export interface Capture {
  target: string;
  source: string;
  nodes: Node[];
  warnings: string[];
}
export interface Observation {
  revision: number;
  base_revision: number | null;
  target: string;
  source: string;
  mode: "full" | "diff" | "unchanged";
  text: string;
  node_count: number;
  truncated: boolean;
  warnings: string[];
}
export function clip(value: string, max: number): string {
  const chars = Array.from(value.replace(/\s+/gu, " ").trim());
  return chars.length > max
    ? chars.slice(0, max).join("") + "…"
    : chars.join("");
}
/** One renderer belongs to one session. References are never recycled. */
export class Renderer {
  revision = 0;
  private next = 0;
  private ids = new Map<string, string>();
  private previous: [string, string][] = [];
  private target = "";
  private current = new Map<string, Node>();
  private valid = false;
  invalidate() {
    this.valid = false;
  }
  reset() {
    this.invalidate();
    this.ids.clear();
    this.previous = [];
    this.current.clear();
  }
  resolve(revision: number, reference: string): Node {
    if (!this.valid || revision !== this.revision)
      throw new Error("STALE_OBSERVATION: observe again before acting");
    const node = this.current.get(reference);
    if (!node) throw new Error(`Unknown element reference: ${reference}`);
    return node;
  }
  render(capture: Capture, limits: Limits, full = false): Observation {
    limitsFor(limits);
    this.invalidate();
    if (capture.target !== this.target) {
      this.reset();
      this.target = capture.target;
    }
    this.current.clear();
    const parents = new Map<string, string | undefined>(),
      seen = new Set<string>();
    const names = new Map<string, string>();
    const lines: [string, string][] = [],
      warnings = [...capture.warnings];
    let truncated = warnings.length > 0,
      bytes = 0,
      visited = 0;
    for (const raw of capture.nodes) {
      if (++visited > limits.maxNodes) {
        truncated = true;
        break;
      }
      if (seen.has(raw.key)) {
        warnings.push("Duplicate/cyclic element skipped");
        truncated = true;
        continue;
      }
      seen.add(raw.key);
      const node = {
        ...raw,
        parent: raw.parent ? parents.get(raw.parent) : undefined,
        states: [...new Set(raw.states)].sort(),
        actions: [...new Set(raw.actions)].sort(),
      };
      if (node.protected) {
        node.value = "";
        node.states = [...new Set([...node.states, "protected"])].sort();
      }
      const duplicateText =
        ["StaticText", "InlineTextBox"].includes(node.role) &&
        !node.actions.length &&
        !node.states.length &&
        !!node.name &&
        node.parent &&
        names.get(node.parent) === node.name;
      if (duplicateText) {
        parents.set(node.key, node.parent);
        continue;
      }
      const generic = [
        "generic",
        "group",
        "none",
        "ignored",
        "InlineTextBox",
      ].includes(node.role);
      if (
        generic &&
        !node.name &&
        !node.value &&
        !node.actions.length &&
        !node.states.length
      ) {
        parents.set(node.key, node.parent);
        continue;
      }
      parents.set(node.key, node.key);
      names.set(node.key, node.name);
      let id = this.ids.get(node.key);
      if (!id) {
        id = `e${++this.next}`;
        this.ids.set(node.key, id);
      }
      const parent = node.parent && this.ids.get(node.parent);
      let line = `[${id}]${parent ? ` parent=${parent}` : ""} ${clip(node.role, 80)}`;
      if (node.name)
        line += ` ${JSON.stringify(clip(node.name, limits.maxTextChars))}`;
      if (node.value && node.value !== node.name)
        line += ` value=${JSON.stringify(clip(node.value, limits.maxTextChars))}`;
      if (node.states.length) line += ` (${node.states.join(",")})`;
      if (node.actions.length) line += ` actions=${node.actions.join(",")}`;
      if (bytes + Buffer.byteLength(line) + 1 > limits.maxOutputBytes - 64) {
        truncated = true;
        break;
      }
      bytes += Buffer.byteLength(line) + 1;
      lines.push([id, line]);
      this.current.set(id, node);
    }
    let text = lines.map(([, line]) => line).join("\n");
    if (truncated)
      text += "\n[truncated: narrow the target or increase observation limits]";
    if (!lines.length && !truncated) text = "[no accessible elements]";
    let mode: Observation["mode"] = "full",
      base_revision: number | null = null;
    if (
      !full &&
      this.revision &&
      !truncated &&
      lines.length === this.previous.length &&
      lines.every(([id], i) => id === this.previous[i][0])
    ) {
      const changed = lines.filter(
        ([, line], i) => line !== this.previous[i][1],
      );
      const delta = changed.length
        ? changed.map(([, line]) => `~ ${line}`).join("\n")
        : "[unchanged]";
      if (Buffer.byteLength(delta) < Buffer.byteLength(text)) {
        text = delta;
        mode = changed.length ? "diff" : "unchanged";
        base_revision = this.revision;
      }
    }
    const keys = new Set([...this.current.values()].map((n) => n.key));
    for (const key of this.ids.keys()) if (!keys.has(key)) this.ids.delete(key);
    this.previous = lines;
    this.valid = true;
    return {
      revision: ++this.revision,
      base_revision,
      target: capture.target,
      source: capture.source,
      mode,
      text,
      node_count: lines.length,
      truncated,
      warnings,
    };
  }
}
