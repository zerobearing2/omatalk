# The [package] version in a Cargo.toml, never a dependency's.
cargo_version() {
  local manifest="$1"
  local version
  version="$(awk '
    /^\[/ {
      in_package = ($0 == "[package]")
    }
    in_package && /^version = "/ {
      sub(/^version = "/, "")
      sub(/".*$/, "")
      print
      exit
    }
  ' "$manifest")"
  if [ -z "$version" ]; then
    echo "could not read the [package] version from $manifest" >&2
    return 1
  fi
  printf '%s\n' "$version"
}

installer_pin() {
  local script="$1"
  local name="$2"
  local line
  while IFS= read -r line; do
    case "$line" in
      "$name=\""*)
        line="${line#"$name=\""}"
        printf '%s\n' "${line%\"}"
        return 0
        ;;
    esac
  done < "$script"
  echo "$script has no $name= pin" >&2
  return 1
}
