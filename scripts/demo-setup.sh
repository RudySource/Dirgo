#!/usr/bin/env bash
set -euo pipefail

# Use a stable public temp prefix so the rendered demo never captures a
# machine- or user-specific TMPDIR path.
demo_root=$(mktemp -d "/private/tmp/dirgo-demo.XXXXXX")
fixture_root="$demo_root/filesystem"
config_home="$demo_root/config"
cache_home="$demo_root/cache"
state_home="$demo_root/state"

mkdir -p \
  "$fixture_root/Projects/Punk/api" \
  "$fixture_root/Projects/Portal/api" \
  "$fixture_root/Archive" \
  "$config_home/dirgo"
printf '%s\n' '[package]' 'name = "punk-demo"' > "$fixture_root/Projects/Punk/Cargo.toml"
printf 'schema_version = 1\nroots = ["%s"]\n' "$fixture_root" > "$config_home/dirgo/config.toml"

printf 'export DGO_DEMO_ROOT=%q\n' "$demo_root"
printf 'export XDG_CONFIG_HOME=%q\n' "$config_home"
printf 'export XDG_CACHE_HOME=%q\n' "$cache_home"
printf 'export XDG_STATE_HOME=%q\n' "$state_home"
printf 'cd %q\n' "$fixture_root"
