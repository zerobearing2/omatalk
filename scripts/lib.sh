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

PLUGIN_ID="zerobearing.omatalk"
PLUGIN_FILES=(manifest.json BarWidget.qml Panel.qml LICENSE)

stage_plugin() {
  local root="$1"
  local dest="$2"
  local version="$3"
  local source="$root/plugin/$PLUGIN_ID"
  local target="$dest/$PLUGIN_ID"
  local version_line='^( *"version": ")[^"]*(",?)$'
  local file
  local line
  local stamped=0

  mkdir -p "$target"
  for file in "${PLUGIN_FILES[@]}"; do
    if [ ! -f "$source/$file" ]; then
      echo "missing plugin file $source/$file" >&2
      return 1
    fi
    cp "$source/$file" "$target/$file"
  done

  : > "$target/manifest.json"
  while IFS= read -r line; do
    if [[ "$line" =~ $version_line ]]; then
      line="${BASH_REMATCH[1]}$version${BASH_REMATCH[2]}"
      stamped=1
    fi
    printf '%s\n' "$line" >> "$target/manifest.json"
  done < "$source/manifest.json"
  if [ "$stamped" -ne 1 ]; then
    echo "no version line in $source/manifest.json" >&2
    return 1
  fi
}
