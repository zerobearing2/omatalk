# Bash style

Apply these rules to every bash or shell script you write or edit. Write the
boring way, for readability and easy debugging.

1. Use a full `if` / `then` / `else` / `fi` block for every guard, exit, and
   return. Leave `&&` and `||` out of control flow.
2. Put each clause of a conditional on its own line.
3. Expand every loop across multiple indented lines.
4. Prefer plain, well-known Unix tools and flags over dense `sed` or `awk`
   chains, deep pipelines, and obscure flag combinations.

## Examples

```bash
# bad
[ -d "logs" ] || mkdir "logs"
[ ! -f "config.json" ] && echo "Error" && exit 1
[ -x "./setup.sh" ] && ./setup.sh || exit 1
for f in *.log; do mv "$f" "${f%.log}.bak"; done
check_status() {
  systemctl is-active --quiet nginx && return 0 || return 1
}
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

for f in *.log; do
  mv "$f" "${f%.log}.bak"
done

check_status() {
  if systemctl is-active --quiet nginx; then
    return 0
  else
    return 1
  fi
}
```
