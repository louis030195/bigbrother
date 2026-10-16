# Observation and action runtime

The architecture follows the observable computer-use patterns studied in the Codex desktop runtime: platform accessibility adapters, bounded semantic text, revisions, optional vision, and actions tied to observed identities. This repository implements those patterns independently; it does not bundle Codex executables, extracted JavaScript or vendor WebAssembly.

## Session contract

Both native and browser sessions emit:

- `revision`: monotonically increasing within one session.
- `base_revision`: the prior revision required to interpret a diff; null for full output.
- `mode`: `full`, `diff`, or `unchanged`.
- `text`: indexed semantic lines. A child carries `parent=eN`. Changed lines start with `~`.
- `target`, `source`, `node_count`, `truncated`, and `warnings`.

Keep the previous full state and apply changed lines by element reference. Structural changes return a new full state, preserving child order without a complex patch language. Diffs are used only when shorter than full text. Request `full:true` after losing your baseline.

References are opaque session identities, not traversal offsets. They are never recycled within the session. Navigation resets browser identity mappings. Every action requires the latest revision, validates its target again and invalidates that observation. A stale revision, detached element, changed label/value/bounds, unsupported action or ambiguous selector fails instead of selecting a different control.

Providers can change between validation and input. The browser resolves an exact DOM object and uses a temporary unique attribute to give Playwright that same target, removing the attribute afterward. Native actions hold the freshly resolved AX/UIA handle. This narrows the race; it is not a transaction with the target application.

## Context efficiency

The default limits are 2,000 visited nodes, depth 32, 200 children per element, 500 Unicode characters per label/value and 32,000 rendered bytes. Limit names in JSON are `maxNodes`, `maxDepth`, `maxChildren`, `maxTextChars`, `maxOutputBytes`. Empty structural wrappers are flattened while preserving parent relationships. Browser static-text children repeating their parent's label are removed. Protected input values are suppressed. Truncation is explicit and partial snapshots are not emitted as diffs.

These limits constrain traversal/rendering and model-facing output. Chromium still supplies its raw AX/DOM responses before local filtering, and an OS accessibility provider can block. Native capture checks a five-second traversal budget between provider calls; macOS requests a short messaging timeout and Windows configures UIA connection/transaction timeouts when available. This is not a hard end-to-end real-time guarantee.

Windows UIA batches name, control type, bounds, enabled, focus and password properties using cache requests. Runtime IDs identify elements. macOS uses AX object equality to retain identities between observations. The renderer is pure Rust on native platforms and TypeScript in the browser runtime; neither requires a model call.

## Platform behavior

| Capability | Native macOS | Native Windows | Chromium browser |
|---|---|---|---|
| Observation provider | AX | UI Automation | CDP AX + DOM metadata |
| Stable references/diffs | Yes | Yes | Yes |
| Semantic click | AXPress | InvokePattern | Playwright exact observed target |
| Set value | AXValue | ValuePattern | Playwright fill |
| Expand/collapse | Not advertised | Where UIA exposes the pattern | Use supported page controls |
| Screenshot | Existing `bb screenshot` command | Existing `bb screenshot` command | Session `screenshot` method |
| Guarded coordinate input | Not in native session protocol | Not in native session protocol | Screenshot-bound `click_at` |

The Windows renderer intentionally supports the shared revision format even though the inspected Codex Windows native adapter returned full states. This is an implementation choice, not a claim of identical behavior.

The browser visits same-process iframe trees through their page CDP session and attaches separate sessions for out-of-process frames. An unavailable frame produces a warning rather than silently claiming a complete capture. Native support does not implement Linux AT-SPI. Browser observation currently requires Chromium/CDP; `snapshot` with `raw:true` remains available for plain Playwright ARIA output without actionable references.

## Browser screenshot fallback

```json
{"id":1,"method":"screenshot","params":{"path":"/tmp/bb-shot.png"}}
{"id":2,"method":"click_at","params":{"screenshot_id":"returned-id","x":120,"y":80}}
```

Viewport screenshots use CSS-pixel coordinates and return width/height. Coordinate input requires the latest screenshot ID, unchanged navigation epoch, viewport dimensions and viewport pixels. The screenshot is consumed by an attempt. Animation can therefore require recapture. Full-page screenshots are observation-only and do not authorize coordinate input. Any accessibility observation or other action clears the screenshot mapping.

## Transport and packaging

Native `bb serve --app …` uses serialized newline-delimited JSON. Browser stdio and per-site daemon calls share the same per-page action queue. Request objects are shape-checked and capped at 1 MiB. EOF drains completed stdio work before closing. Unix sockets use a per-user private temporary directory; Windows uses named pipes. Site names cannot traverse directories.

The Rust `web` command forwards arguments to the same Bun CLI used by npm; its obsolete second command schema and Node-only launcher have been removed.

The npm browser payload is generated by `npm/bb/scripts/prepare-browser.cjs` from `browser/src`; tests and development fixtures are excluded. The previous committed duplicate source and broken `browser/src/cli.ts` installed path have been removed.

## Migration and validation limits

`bb tree` and browser `snap` now produce the revisioned format. Consumers of old output must use native `tree --raw` or browser `snap --raw`. Exact-name/strict locator behavior may expose previously ambiguous workflows; use a more precise target rather than relying on the first match. Implicit cookie seeding and forced removal of Chrome profile locks are gone; login/seed are explicit operations.

The headless integration suite exercises actual Chromium capture/actions, Unicode input, same-process and cross-site iframes, stale-target rejection, screenshots, request ordering and non-destructive text observation. Rust tests cover rendering and native action validation with a fake provider. macOS workspace tests and Windows cross-compilation do not establish live Windows UI behavior. Existing native screenshot implementations have not been replaced with Codex's ScreenCaptureKit/Windows Graphics Capture stack.
