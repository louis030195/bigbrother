---
name: wezterm-orchestrator
description: Control WezTerm panes - list panes, switch between them, and type commands into specific panes. Use when coordinating multiple terminal sessions or agents running in different panes.
---

# WezTerm Orchestrator

Control multiple WezTerm panes programmatically using `bb wezterm` commands.

## List all panes

```bash
bb wezterm list
```

Returns JSON with pane_id, title, cwd, is_active for each pane.

## Send text to a specific pane

```bash
bb wezterm send <pane_id> "your text here"
```

This activates the pane, types the text, and presses enter.

Add `--no-enter` to skip pressing enter:
```bash
bb wezterm send <pane_id> "partial text" --no-enter
```

## Focus a pane (without typing)

```bash
bb wezterm focus <pane_id>
```

## Examples

### Send a prompt to pane 0
```bash
bb wezterm send 0 "list all files in the current directory"
```

### Check pane layout
```bash
bb wezterm list | jq '.data[] | {pane_id, title, cwd}'
```

### Send to all Pi panes
```bash
for id in $(bb wezterm list | jq -r '.data[] | select(.title | contains("π")) | .pane_id'); do
  bb wezterm send $id "check for errors"
done
```

## Notes

- Pane IDs are stable within a session
- Use `bb wezterm list` to discover pane IDs
- The `bb` binary is at `~/Documents/superintelligence/target/release/bb`
