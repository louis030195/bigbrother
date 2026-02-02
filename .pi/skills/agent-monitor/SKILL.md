---
name: agent-monitor
description: Monitor all Pi agents in WezTerm. Shows table with pane, project, status, task, and current action. Use to coordinate multiple agents.
---

# Agent Monitor

Show status table for all Pi agents.

## Run Status Check

```bash
echo "🧠 AGENT STATUS - $(date '+%H:%M:%S')"
echo ""
echo "| Pane | Project | Status | Task | Doing |"
echo "|------|---------|--------|------|-------|"

PANES=$(bb wezterm list 2>/dev/null)
if [ $? -ne 0 ] || [ -z "$PANES" ]; then
  echo "| - | - | ⚠️ | WezTerm not accessible | - |"
  exit 0
fi

echo "$PANES" | jq -r '.data[] | "\(.pane_id)|\(.title)|\(.cwd)"' | while IFS='|' read -r PANE_ID TITLE CWD; do
  CWD_CLEAN=$(echo "$CWD" | sed 's|file://||')
  PROJECT=$(echo "$CWD_CLEAN" | xargs basename 2>/dev/null || echo "unknown")
  
  if [[ "$TITLE" != *"π"* ]]; then
    echo "| $PANE_ID | $PROJECT | ⚙️ other | - | - |"
    continue
  fi
  
  SESSION_PATH=$(echo "$CWD_CLEAN" | sed 's|/|-|g' | sed 's|^-||')
  SESSION=$(ls -t "$HOME/.pi/agent/sessions/--${SESSION_PATH}--"/*.jsonl 2>/dev/null | head -1)
  
  if [ -z "$SESSION" ]; then
    echo "| $PANE_ID | $PROJECT | ❓ | no session | - |"
    continue
  fi
  
  STOP=$(tail -1 "$SESSION" | jq -r '.message.stopReason // "working"')
  ROLE=$(tail -1 "$SESSION" | jq -r '.message.role // "unknown"')
  [ "$STOP" = "stop" ] && [ "$ROLE" = "assistant" ] && STATUS="✅ idle" || STATUS="🔄 working"
  
  TASK=$(tail -100 "$SESSION" | jq -r 'select(.type=="message" and .message.role=="user") | .message.content | if type=="array" then .[0].text else . end' 2>/dev/null | tail -1 | cut -c1-25)
  [ -z "$TASK" ] && TASK="-"
  
  DOING=$(tail -20 "$SESSION" | jq -r 'select(.type=="message" and .message.role=="assistant") | .message.content | if type=="array" then [.[] | select(.type=="toolCall") | .name] | join(",") else "thinking" end' 2>/dev/null | tail -1)
  [ -z "$DOING" ] && DOING="-"
  
  echo "| $PANE_ID | $PROJECT | $STATUS | $TASK | $DOING |"
done
```

## Columns

- **Pane**: WezTerm pane ID
- **Project**: Directory name
- **Status**: ✅ idle (ready) / 🔄 working / ⚙️ other (not Pi)
- **Task**: Last user prompt (truncated)
- **Doing**: Current tool (bash, read, edit, write) or "-"

## After Status Check

1. **✅ idle agents** → Assign task: `bb wezterm send <pane> "task"`
2. **🔄 working agents** → Wait or monitor
3. **Multiple agents same task** → Possible duplicate work
4. **Stuck agent** → `bb wezterm send <pane> "status?"`
