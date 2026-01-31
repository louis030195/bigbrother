# workflow-recorder

High-performance macOS workflow recorder using CGEventTap and Accessibility APIs.

## Features

- **Efficient**: 3 lightweight threads - event tap, app observer, clipboard monitor
- **Complete capture**: Clicks, mouse moves, scrolls, keyboard, app switches, clipboard
- **AI-friendly output**: Compact JSON lines format with minimal fields
- **Replay**: Full playback using CGEventPost
- **Context capture**: UI element info at click positions (optional)

## Requirements

- macOS 10.15+
- Accessibility permission
- Input Monitoring permission

## CLI

```bash
# Check permissions
wr permissions

# Record (Ctrl+C to stop)
wr record -n my-workflow

# Record without element context (faster)
wr record -n fast --no-context

# List recordings
wr list

# Show recording info
wr show my-workflow_20240101_120000.jsonl

# Replay at 2x speed
wr replay my-workflow_20240101_120000.jsonl -s 2.0

# Delete
wr delete my-workflow_20240101_120000.jsonl
```

## Output Format

Events are stored as JSON lines (`.jsonl`). Each event has:
- `t`: timestamp in ms since start
- `e`: event type
- Type-specific fields

```json
{"t":100,"e":"c","x":500,"y":300,"b":0,"n":1,"m":0}
{"t":150,"e":"m","x":520,"y":310}
{"t":200,"e":"k","k":0,"m":8}
{"t":250,"e":"t","s":"hello world"}
{"t":300,"e":"a","n":"Safari","p":1234}
{"t":350,"e":"p","o":"c","s":"copied text"}
{"t":400,"e":"x","r":"AXButton","n":"Submit"}
```

Event types:
- `c`: click (x, y, button, clicks, modifiers)
- `m`: mouse move (x, y)
- `s`: scroll (x, y, dx, dy)
- `k`: key press (keycode, modifiers)
- `t`: text input (aggregated string)
- `a`: app switch (name, pid)
- `p`: clipboard (operation, preview)
- `x`: context (role, name, value)

## Library Usage

```rust
use workflow_recorder::prelude::*;

// Record
let recorder = WorkflowRecorder::new();
let (mut workflow, handle) = recorder.start("demo")?;

// ... wait for user actions ...

handle.drain(&mut workflow);
handle.stop(&mut workflow);

// Save
let storage = WorkflowStorage::new()?;
storage.save(&workflow)?;

// Replay
let workflow = storage.load("demo_20240101.jsonl")?;
let replayer = Replayer::new().speed(2.0);
replayer.play(&workflow)?;
```

## Architecture

Three parallel threads:
1. **Event Tap** (CGEventTap): Mouse/keyboard capture, runs CFRunLoop
2. **App Observer** (NSWorkspace): App activation notifications
3. **Clipboard Monitor**: Polls pbpaste every 250ms

Events flow through a lock-free channel to the main thread.
