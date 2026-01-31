# superintelligence

macOS desktop automation for AI agents. Deterministic Rust primitives with structured JSON output for AI recovery.

## Philosophy

1. **Deterministic first**: Write Rust scripts that run at CPU speed
2. **Structured errors**: When things fail, AI gets JSON with context + suggestions
3. **AI recovery**: Agent writes more Rust code to fix failures

## CLI

```bash
# List running apps
si apps

# Get accessibility tree (for AI to understand what's on screen)
si tree --app Arc --depth 15

# Find elements
si find "role:Button AND name:Submit" --app Arc
si find "name~:screenpipe" --app Discord  # contains match

# Click
si click "role:Button AND name:Submit"
si click "index:42"  # from last tree

# Type
si type "hello world"
si type "hello" --selector "role:TextField" --app Arc

# Scroll
si scroll --direction up --pages 5 --app Arc

# Scrape all text
si scrape --app Arc --depth 20

# Wait
si wait --idle 3000
si wait --selector "name~:Loading" --timeout 10000
```

All commands output JSON:
```json
{
  "success": true,
  "data": { ... }
}
```

On failure:
```json
{
  "success": false,
  "error": {
    "code": "ELEMENT_NOT_FOUND",
    "message": "No element matching: role:Button AND name:Login",
    "suggestions": ["Try: role:Button AND name:Sign In"],
    "context": { "visible_buttons": [...] }
  }
}
```

## Library

```rust
use superintelligence::prelude::*;

fn main() -> Result<()> {
    let desktop = Desktop::new()?;

    // Find and interact
    desktop.locator("role:Button AND name:Submit")?
        .timeout(5000)
        .click()?;

    // Or chain it
    desktop.locator("name~:screenpipe")?
        .click()?;

    // Scrape
    let result = desktop.scrape("Arc", 20)?;
    for item in result.items {
        println!("{}: {}", item.role, item.text);
    }

    // Scroll loop
    for _ in 0..10 {
        desktop.scroll_up(5)?;
        desktop.wait_idle(1000)?;
    }

    Ok(())
}
```

## Selector Syntax

```
role:Button              # exact role match
name:Submit              # exact name match
name~:screenpipe         # name contains (case-insensitive)
title:Login              # exact title match
value~:hello             # value contains
index:42                 # element by index from last tree

# Compound (AND)
role:Button AND name:Submit
role:TextField AND name~:search
```

## AI Agent Workflow

1. AI calls `si tree --app Arc` to see what's on screen
2. AI calls `si click "role:Button AND name:Login"`
3. If fails, AI reads error JSON with suggestions
4. AI either retries with suggestion or writes custom Rust script
5. Repeat

## Setup

```bash
cargo install --path .

# Grant accessibility permissions when prompted
si apps
```

## License

MIT
