// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
import type { CDPSession, Page } from "playwright";
import { randomUUID, createHash } from "node:crypto";
import { extname } from "node:path";
import {
  Renderer,
  limitsFor,
  defaults,
  type Capture,
  type Limits,
  type Node,
} from "./observation.js";
type AXNode = {
  nodeId: string;
  backendDOMNodeId?: number;
  childIds?: string[];
  ignored?: boolean;
  role?: { value?: string };
  name?: { value?: string };
  value?: { value?: unknown };
  properties?: { name: string; value: { value?: unknown } }[];
};
type Target = { cdp: CDPSession; backend: number };
type Captured = Capture & { targets: Map<string, Target> };
const actionable = new Set([
  "button",
  "link",
  "checkbox",
  "radio",
  "tab",
  "menuitem",
  "menuitemcheckbox",
  "menuitemradio",
  "combobox",
  "textbox",
  "searchbox",
  "switch",
  "slider",
  "spinbutton",
]);
export class BrowserRuntime {
  readonly renderer = new Renderer();
  private sessions: CDPSession[] = [];
  private limits: Limits = defaults;
  private epoch = 0;
  private tail: Promise<unknown> = Promise.resolve();
  private shot?: {
    id: string;
    epoch: number;
    width: number;
    height: number;
    hash: string;
    type: "png" | "jpeg";
  };
  constructor(readonly page: Page) {
    page.on("framenavigated", () => {
      this.epoch++;
      this.renderer.reset();
      this.shot = undefined;
    });
    page.on("close", () => {
      this.renderer.invalidate();
      void this.detach();
    });
  }
  /** Every transport shares this queue so observation and action cannot interleave. */
  run<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.tail.then(operation);
    this.tail = result.catch(() => undefined);
    return result;
  }
  invalidate() {
    this.renderer.invalidate();
    this.shot = undefined;
  }
  private async detach() {
    const sessions = this.sessions;
    this.sessions = [];
    await Promise.all(sessions.map((s) => s.detach().catch(() => {})));
  }
  async observe(options: { limits?: Partial<Limits>; full?: boolean } = {}) {
    const limits = limitsFor(options.limits);
    this.invalidate();
    const captured = await this.capture(limits);
    this.limits = limits;
    return this.renderer.render(captured, limits, options.full);
  }
  private async capture(limits: Limits): Promise<Captured> {
    await this.detach();
    const epoch = this.epoch;
    const main = await this.page.context().newCDPSession(this.page);
    this.sessions.push(main);
    for (const frame of this.page
      .frames()
      .filter((f) => f !== this.page.mainFrame())) {
      try {
        this.sessions.push(await this.page.context().newCDPSession(frame));
      } catch {
        /* same-process frame */
      }
    }
    const roots = await Promise.all(
      this.sessions.map(async (cdp) => ({
        cdp,
        tree: (await cdp.send("Page.getFrameTree")).frameTree,
      })),
    );
    const ownRoots = new Set(roots.map((r) => r.tree.frame.id));
    const nodes: Node[] = [],
      targets = new Map<string, Target>(),
      warnings: string[] = [];
    for (const { cdp, tree } of roots) {
      const metadata = new Map<
        number,
        { password: boolean; bounds?: number[] }
      >();
      try {
        const dom = await cdp.send("DOMSnapshot.captureSnapshot", {
          computedStyles: [],
          includeDOMRects: true,
        });
        for (const doc of dom.documents) {
          const layout = new Map(
            doc.layout.nodeIndex.map((index, i) => [
              index,
              doc.layout.bounds[i],
            ]),
          );
          doc.nodes.backendNodeId?.forEach((backend, i) => {
            const attrs = doc.nodes.attributes?.[i] ?? [];
            let password = false;
            for (let j = 0; j < attrs.length; j += 2)
              if (
                dom.strings[attrs[j]] === "type" &&
                dom.strings[attrs[j + 1]].toLowerCase() === "password"
              )
                password = true;
            metadata.set(backend, { password, bounds: layout.get(i) });
          });
        }
      } catch {
        warnings.push(
          "DOM layout metadata unavailable; coordinate fallback requires a screenshot",
        );
      }
      const frameTrees = [tree];
      while (frameTrees.length) {
        const branch = frameTrees.shift()!;
        frameTrees.push(
          ...(branch.childFrames ?? []).filter(
            (f) => !ownRoots.has(f.frame.id),
          ),
        );
        if (nodes.length >= limits.maxNodes) {
          warnings.push("node limit reached");
          break;
        }
        let ax: AXNode[];
        try {
          ax = (
            await cdp.send("Accessibility.getFullAXTree", {
              frameId: branch.frame.id,
            })
          ).nodes;
        } catch {
          warnings.push(
            `Accessibility unavailable for frame ${branch.frame.id}`,
          );
          continue;
        }
        const byId = new Map(ax.map((n) => [n.nodeId, n])),
          childIds = new Set(ax.flatMap((n) => n.childIds ?? []));
        const stack = ax
          .filter((n) => !childIds.has(n.nodeId))
          .reverse()
          .map((n) => ({
            node: n,
            parent: undefined as string | undefined,
            depth: 0,
          }));
        const seen = new Set<string>();
        while (stack.length) {
          const { node: raw, parent, depth } = stack.pop()!;
          if (seen.has(raw.nodeId)) {
            warnings.push("AX cycle skipped");
            continue;
          }
          seen.add(raw.nodeId);
          if (nodes.length >= limits.maxNodes) {
            warnings.push("node limit reached");
            break;
          }
          const key = `${epoch}:${branch.frame.id}:${raw.backendDOMNodeId ?? `ax${raw.nodeId}`}`;
          const properties = new Map(
            (raw.properties ?? []).map((p) => [p.name, p.value.value]),
          );
          const role = raw.ignored ? "ignored" : (raw.role?.value ?? "generic");
          const meta = raw.backendDOMNodeId
            ? metadata.get(raw.backendDOMNodeId)
            : undefined;
          const protectedValue =
            meta?.password || properties.get("protected") === true;
          const states = [
            "disabled",
            "focused",
            "selected",
            "checked",
            "expanded",
            "readonly",
            "required",
            "invalid",
          ]
            .flatMap((name) => {
              const value = properties.get(name);
              return value === true ||
                (value !== undefined &&
                  value !== false &&
                  value !== "false" &&
                  value !== "none")
                ? [`${name}${value === true ? "" : `=${value}`}`]
                : [];
            })
            .sort();
          const actions =
            !raw.ignored &&
            raw.backendDOMNodeId &&
            actionable.has(role) &&
            !properties.get("disabled")
              ? ["click"]
              : [];
          if (
            actions.length &&
            ["textbox", "searchbox", "combobox", "spinbutton"].includes(role) &&
            !properties.get("readonly")
          )
            actions.push("set_value");
          actions.sort();
          nodes.push({
            key,
            parent,
            role,
            name: raw.ignored ? "" : (raw.name?.value ?? ""),
            value:
              protectedValue || raw.ignored
                ? ""
                : String(raw.value?.value ?? ""),
            states,
            actions,
            protected: protectedValue,
            bounds: meta?.bounds,
          });
          if (raw.backendDOMNodeId)
            targets.set(key, { cdp, backend: raw.backendDOMNodeId });
          const children = raw.childIds ?? [];
          if (depth >= limits.maxDepth) {
            if (children.length) warnings.push("depth limit reached");
            continue;
          }
          if (children.length > limits.maxChildren)
            warnings.push("child limit reached");
          for (const id of children.slice(0, limits.maxChildren).reverse()) {
            const child = byId.get(id);
            if (child)
              stack.push({ node: child, parent: key, depth: depth + 1 });
          }
        }
      }
    }
    if (epoch !== this.epoch)
      throw new Error("Page navigated while capturing; observe again");
    return {
      target: `${epoch}:${this.page.url()}`,
      source: "chromium_ax_dom",
      nodes,
      targets,
      warnings: [...new Set(warnings)],
    };
  }
  async act(params: {
    revision: number;
    reference: string;
    action: string;
    value?: string;
  }) {
    const expected = this.renderer.resolve(params.revision, params.reference);
    this.invalidate();
    if (!expected.actions.includes(params.action))
      throw new Error(
        `Action ${params.action} was not advertised for this element`,
      );
    const captured = await this.capture(this.limits),
      current = captured.nodes.find((n) => n.key === expected.key);
    const signature = (n: Node) =>
      JSON.stringify([
        n.parent,
        !!n.protected,
        n.role,
        n.name,
        n.protected ? "" : n.value,
        n.bounds,
        n.actions,
        n.states.filter((s) => s !== "protected"),
      ]);
    if (!current || signature(current) !== signature(expected))
      throw new Error(
        "STALE_OBSERVATION: element changed or disappeared; observe again",
      );
    const target = captured.targets.get(expected.key);
    if (!target) throw new Error("Element has no actionable DOM target");
    const { object } = await target.cdp.send("DOM.resolveNode", {
      backendNodeId: target.backend,
    });
    if (!object.objectId) throw new Error("Element is detached; observe again");
    const attribute = `data-bb-${randomUUID()}`;
    try {
      const tagged = await target.cdp.send("Runtime.callFunctionOn", {
        objectId: object.objectId,
        functionDeclaration:
          "function(attr) { if (!this.isConnected || this.nodeType !== 1) return false; this.setAttribute(attr, ''); return true; }",
        arguments: [{ value: attribute }],
        returnByValue: true,
      });
      if (tagged.result.value !== true)
        throw new Error("Element is detached; observe again");
      const matches = [];
      for (const frame of this.page.frames()) {
        const locator = frame.locator(`[${attribute}]`);
        if (await locator.count()) matches.push(locator);
      }
      if (matches.length !== 1)
        throw new Error("Could not uniquely resolve the observed element");
      if (params.action === "click") await matches[0].click({ timeout: 5000 });
      else if (params.action === "set_value") {
        if (typeof params.value !== "string")
          throw new Error("set_value requires a string value");
        await matches[0].fill(params.value, { timeout: 5000 });
      } else throw new Error("Unsupported action");
      return { acted: true, observe_required: true };
    } finally {
      await target.cdp
        .send("Runtime.callFunctionOn", {
          objectId: object.objectId,
          functionDeclaration: "function(attr) { this.removeAttribute(attr); }",
          arguments: [{ value: attribute }],
        })
        .catch(() => {});
      await target.cdp
        .send("Runtime.releaseObject", { objectId: object.objectId })
        .catch(() => {});
    }
  }
  async screenshot(path?: string) {
    this.invalidate();
    const epoch = this.epoch;
    const size = await this.page.evaluate(() => ({
      width: innerWidth,
      height: innerHeight,
    }));
    const type = /\.jpe?g$/i.test(extname(path ?? "")) ? "jpeg" : "png";
    const buffer = await this.page.screenshot({ path, scale: "css" });
    if (epoch !== this.epoch)
      throw new Error("Page navigated during screenshot; capture again");
    this.shot = {
      id: randomUUID(),
      epoch,
      ...size,
      type,
      hash: createHash("sha256").update(buffer).digest("hex"),
    };
    return {
      screenshot_id: this.shot.id,
      width: size.width,
      height: size.height,
      coordinate_space: "viewport_css_pixels",
      path: path ?? null,
      ...(path ? {} : { base64: buffer.toString("base64") }),
    };
  }
  async clickAt(params: { screenshot_id: string; x: number; y: number }) {
    const shot = this.shot;
    this.invalidate();
    const size = await this.page.evaluate(() => ({
      width: innerWidth,
      height: innerHeight,
    }));
    if (
      !shot ||
      shot.id !== params.screenshot_id ||
      shot.epoch !== this.epoch ||
      size.width !== shot.width ||
      size.height !== shot.height
    )
      throw new Error(
        "STALE_SCREENSHOT: capture again before coordinate input",
      );
    if (
      !Number.isFinite(params.x) ||
      !Number.isFinite(params.y) ||
      params.x < 0 ||
      params.y < 0 ||
      params.x >= shot.width ||
      params.y >= shot.height
    )
      throw new Error("Coordinates are outside the captured viewport");
    const currentImage = await this.page.screenshot({
      scale: "css",
      type: shot.type,
    });
    // Navigation can finish while screenshot capture is awaiting the browser.
    // Matching returned pixels alone must never authorize input into the new page.
    if (
      shot.epoch !== this.epoch ||
      createHash("sha256").update(currentImage).digest("hex") !== shot.hash
    )
      throw new Error(
        "STALE_SCREENSHOT: viewport contents changed; capture again",
      );
    await this.page.mouse.click(params.x, params.y);
    return { acted: true, observe_required: true };
  }
}
const runtimes = new WeakMap<Page, BrowserRuntime>();
export function runtimeFor(page: Page): BrowserRuntime {
  let runtime = runtimes.get(page);
  if (!runtime) {
    runtime = new BrowserRuntime(page);
    runtimes.set(page, runtime);
  }
  return runtime;
}
