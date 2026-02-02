---
name: pi-session-reader
description: Read and understand Pi agent sessions from disk. Use to check what other Pi agents are doing, their conversation history, and current state.
---

# Pi Session Reader

Read Pi session files to understand what other agents are doing.

## Session Location

Sessions are stored at:
```
~/.pi/agent/sessions/--<path>--/<timestamp>_<uuid>.jsonl
```

Where `<path>` is the working directory with `/` replaced by `-`.

## Find sessions for a directory

```bash
# List sessions for screenpipe
ls -lt ~/.pi/agent/sessions/--Users-louisbeaumont-Documents-screenpipe--/ | head -5

# List sessions for brain
ls -lt ~/.pi/agent/sessions/--Users-louisbeaumont-Documents-brain--/ | head -5
```

## Read latest session

```bash
# Get most recent session file
LATEST=$(ls -t ~/.pi/agent/sessions/--Users-louisbeaumont-Documents-screenpipe--/*.jsonl | head -1)

# Read last 50 lines (most recent messages)
tail -50 "$LATEST"
```

## Session Format (JSONL)

Each line is JSON with a `type` field:

- `session` - Header with metadata
- `message` - Conversation message with `role` (user/assistant/toolResult)
- `model_change` - Model switch
- `compaction` - Context was compacted

### Message structure

```json
{"type":"message","id":"abc123","parentId":"xyz789","message":{"role":"user","content":"hello"}}
{"type":"message","id":"def456","parentId":"abc123","message":{"role":"assistant","content":[{"type":"text","text":"Hi!"}]}}
```

## Extract conversation summary

To understand what an agent is working on:

```bash
# Get last 20 messages, extract user prompts and assistant text
tail -100 "$SESSION" | jq -r 'select(.type=="message") | .message | select(.role=="user" or .role=="assistant") | "\(.role): \(.content | if type=="array" then .[0].text else . end)"' 2>/dev/null | tail -20
```

## Check if agent is busy

If the last message is `role: user` or `role: toolResult`, the agent is processing.
If the last message is `role: assistant` with `stopReason: stop`, the agent is idle.

```bash
tail -1 "$SESSION" | jq '.message.role, .message.stopReason'
```

## Useful patterns

### Get all recent sessions across projects
```bash
find ~/.pi/agent/sessions -name "*.jsonl" -mmin -60 | while read f; do
  echo "=== $f ==="
  tail -5 "$f" | jq -r '.message.content // .type' 2>/dev/null
done
```

### Get current working task for a session
```bash
tail -20 "$SESSION" | jq -r 'select(.message.role=="user") | .message.content' | tail -1
```
