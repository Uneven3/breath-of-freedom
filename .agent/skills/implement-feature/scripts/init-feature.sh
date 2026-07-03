#!/usr/bin/env bash
# Usage: init-feature.sh [docs-root]
DOCS="${1:-docs}"

mkdir -p "$DOCS/adr" "$DOCS/implement-feature" "$DOCS/implement-quick-feature" "$DOCS/features-recipes" "$DOCS/logs"

if [ ! -f "$DOCS/CONSTITUTION.md" ]; then
  cat > "$DOCS/CONSTITUTION.md" << 'EOF'
# Constitution

> Auto-generated stub. Replace with your project's architecture rules.

## §1 — [Rule name]
[Description]

## §2 — [Rule name]
[Description]
EOF
  echo "CREATED $DOCS/CONSTITUTION.md"
fi

if [ ! -f "$DOCS/ARCHITECTURE-MAP.md" ]; then
  cat > "$DOCS/ARCHITECTURE-MAP.md" << 'EOF'
# Architecture Map

## Built

| Class | Path | Public surface | Constitution clauses |
| ----- | ---- | -------------- | -------------------- |

## Deferred

| Pattern | Activation trigger |
| ------- | ------------------ |
EOF
  echo "CREATED $DOCS/ARCHITECTURE-MAP.md"
fi

echo "DONE"
