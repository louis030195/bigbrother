# bigbrother

High-performance macOS workflow recorder using CGEventTap and Accessibility APIs.

## Features

- **Efficient**: 2 lightweight threads - event tap + app/window observer
- **Complete capture**: Clicks, mouse moves, scrolls, keyboard, app/window switches, clipboard
- **Smart clipboard**: Detects Cmd+C/X/V instead of polling
- **AI-friendly output**: Compact JSON lines format with minimal fields
- **Replay**: Full playback using CGEventPost
- **Context capture**: UI element info at click positions (optional)
- **Streaming API**: Consume events in real-time from other crates

## Requirements

- macOS 10.15+
- Accessibility permission
- Input Monitoring permission

## CLI

```bash
# Check permissions
bb permissions

# Record (Ctrl+C to stop)
bb record -n my-workflow

# Record without element context (faster)
bb record -n fast --no-context

# List recordings
bb list

# Show recording info
bb show my-workflow_20240101_120000.jsonl

# Replay at 2x speed
bb replay my-workflow_20240101_120000.jsonl -s 2.0

# Delete
bb delete my-workflow_20240101_120000.jsonl
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
{"t":310,"e":"w","a":"Safari","w":"GitHub - bigbrother"}
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
- `w`: window focus (app name, window title)
- `p`: clipboard (operation: c=copy, x=cut, v=paste)
- `x`: context (role, name, value)

## Library Usage

```rust
use bigbrother::prelude::*;

// Record to workflow
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

## Streaming API

Consume events in real-time from other crates:

```rust
use bigbrother::prelude::*;

let recorder = WorkflowRecorder::new();
let stream = recorder.stream()?;

// As iterator
for event in stream {
    println!("{:?}", event);
}

// Or via receiver for crossbeam select!
let stream = recorder.stream()?;
let rx = stream.receiver();
loop {
    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(event) => println!("{:?}", event),
        Err(_) => break,
    }
}
```

## Architecture

Two parallel threads:
1. **Event Tap** (CGEventTap): Mouse/keyboard/clipboard capture via CFRunLoop
2. **App Observer**: Polls frontmost app every 100ms for app/window changes

Events flow through a lock-free crossbeam channel.
