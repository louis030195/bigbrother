# BigBrother / bbctl

Accessibility-first computer use for agents: inspect compact UI state, act on an observed element, then inspect again. Rust handles macOS AX and Windows UI Automation. Chromium observations combine CDP accessibility and DOM metadata; Playwright performs browser actions.

```text
Platform adapters → bounded snapshot → compact renderer → revision + element refs
                                              ↑                    ↓
                                      fresh observation ← validated action
```

## Browser sessions

```sh
cd browser && bun install
bunx playwright install chromium
bun run src/cli.ts open demo --url https://example.com
# In another terminal, while the session is running:
bun run src/cli.ts snap demo
bun run src/cli.ts click demo e4 --revision 1
bun run src/cli.ts snap demo
bun run src/cli.ts close demo
```

References and revision numbers above are illustrative: use the actual values returned by `snap`. `open` runs the session daemon in the current terminal. Both the Rust executable and npm wrapper expose the same commands as `bb web …` (Bun is required).

- `snap` returns compact full/diff/unchanged observations. Use `--full` to recover a complete baseline or `--raw` for legacy Playwright ARIA text.
- `click demo e4 --revision 1` and `type demo "text" --ref e4 --revision 1` act on a specific observed element. References require an open session.
- Selector/text commands remain available, with exact names and strict uniqueness. Ambiguous targets fail.
- Persistent profiles reuse their own login state. `bb web login …` opens a visible login window; cookie copying is an explicit `bb web seed …` operation.
- `bb web serve demo` exposes the same runtime over newline-delimited JSON.

## Native sessions

```sh
cargo build -p bb
./target/debug/bb observe --app "Calculator"
./target/debug/bb serve --app "Calculator"
```

On macOS, use an exact running application name and grant Accessibility to the executable. On Windows, use a window-title substring or `pid:1234`; multiple matching windows are rejected. Native support is macOS and Windows. Linux native automation remains unsupported.

`bb tree` now returns the compact observation. `bb tree --raw` preserves the previous platform-specific JSON. Existing recording/replay, selectors and `bb screenshot` commands remain available.

Send one JSON request per line to `serve`:

```json
{"id":1,"method":"observe","params":{"full":true,"limits":{"maxNodes":2000,"maxOutputBytes":32000}}}
{"id":2,"method":"act","params":{"revision":1,"reference":"e4","action":"click"}}
{"id":3,"method":"observe"}
```

`act` requires the current revision and an action advertised on the element. It re-resolves the target against a fresh platform snapshot and consumes the observation even if the action fails. Observe again before another action. `capabilities` reports the runtime interface.

See [the observation protocol and platform details](docs/computer-use.md).

## Development and validation

```sh
cargo test --workspace
cargo check --workspace --target x86_64-pc-windows-msvc
cd browser
bun run typecheck
bun run test
bun run scripts/benchmark.ts
```

The default browser test script uses isolated headless Chromium and local fixtures. Older cookie/profile integration tests remain available separately and are not part of that deterministic suite. No personal browser session is needed.

`browser/src` is the only browser implementation. `npm/bb` generates its browser payload at `npm pack`/`npm publish` time. Do not edit the generated directory. No release is published by running the checks above.
