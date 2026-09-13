# Bash Style

Optimize for human readability and easy debugging over brevity. Avoid clever,
compressed, or obfuscated shell syntax — write it the boring way.

## Rules

1. No short-circuit control flow (`&&` / `||`) to guard, exit, or return.
2. Every conditional is a full `if` / `then` / `else` / `fi` block, one line
   per clause.
3. Every loop is expanded across multiple lines with proper indentation —
   never `for x in y; do z; done` on one line.
4. Prefer plain, well-known Unix tools and flags over dense one-liners
   (`sed`/`awk` chains, deeply nested pipelines, obscure flag combos).

## Examples

### File and directory checks

```bash
# bad
[ -d "logs" ] || mkdir "logs"
[ ! -f "config.json" ] && echo "Error" && exit 1
[ -x "./setup.sh" ] && ./setup.sh || exit 1
```

```bash
# good
if [ ! -d "logs" ]; then
  mkdir "logs"
fi

if [ ! -f "config.json" ]; then
  echo "Error"
  exit 1
fi

if [ -x "./setup.sh" ]; then
  ./setup.sh
else
  exit 1
fi
```

### Conditional flags

```bash
# bad
FULL_CLEAN=true
[ "$FULL_CLEAN" = true ] && rm -rf ./tmp
```

```bash
# good
FULL_CLEAN=true
if [ "$FULL_CLEAN" = true ]; then
  rm -rf ./tmp
fi
```

### Loops

```bash
# bad
for f in *.log; do mv "$f" "${f%.log}.bak"; done
```

```bash
# good
for f in *.log; do
  mv "$f" "${f%.log}.bak"
done
```

### Function returns and status codes

```bash
# bad
check_status() {
  systemctl is-active --quiet nginx && return 0 || return 1
}
```

```bash
# good
check_status() {
  if systemctl is-active --quiet nginx; then
    return 0
  else
    return 1
  fi
}
```
