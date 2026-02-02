# BigBrother - Agent Orchestration

This project provides tools for orchestrating multiple Pi coding agents.

## Quick Commands

### List all terminal panes
```bash
bb wezterm list
```

### Send to a specific pane
```bash
bb wezterm send <pane_id> "your prompt"
```

### Focus a pane
```bash
bb wezterm focus <pane_id>
```

## Available Skills

- `wezterm-orchestrator` - Control WezTerm panes via `bb wezterm` commands
- `pi-session-reader` - Read Pi session files to understand agent state

## When to Orchestrate

Use orchestration when:
- Task spans multiple projects/repos
- Need parallel work across codebases
- Want to coordinate frontend + backend changes
- Need to review work done by another agent

## Building bb

```bash
cd ~/Documents/superintelligence
cargo build --release -p bb
```

Binary: `./target/release/bb`
