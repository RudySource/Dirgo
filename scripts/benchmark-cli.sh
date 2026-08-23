#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/benchmark-cli.sh [--directories N] [--samples N] [--fixture PATH] [--keep] [--report PATH]

Creates an isolated fixture unless --fixture is supplied, then measures a cold
index build and warm CLI work. No existing fixture is modified or removed.
Use N=10000, 100000, 500000, or 1000000 for the release matrix.
EOF
}

directories=10000
samples=5
fixture=''
keep=0
report=''
while (($#)); do
  case "$1" in
    --directories) directories=${2:?--directories requires a value}; shift 2 ;;
    --samples) samples=${2:?--samples requires a value}; shift 2 ;;
    --fixture) fixture=${2:?--fixture requires a value}; shift 2 ;;
    --keep) keep=1; shift ;;
    --report) report=${2:?--report requires a value}; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) printf 'Unknown argument: %s\n\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done
case "$directories" in
  ''|*[!0-9]*) printf '%s\n' '--directories must be a positive integer' >&2; exit 2 ;;
esac
case "$samples" in
  ''|*[!0-9]*|0) printf '%s\n' '--samples must be a positive integer' >&2; exit 2 ;;
esac

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
dgo_bin=${DGO_BIN:-"$repo_root/target/release/dgo"}
fixture_bin=${DGO_FIXTURE_BIN:-"$repo_root/target/release/dgo-fixture"}
[[ -x "$dgo_bin" ]] || { printf 'DGO binary is not executable: %s\n' "$dgo_bin" >&2; exit 2; }
[[ -x "$fixture_bin" ]] || { printf 'Fixture binary is not executable: %s\n' "$fixture_bin" >&2; exit 2; }
if [[ -n "$report" ]]; then
  [[ ! -e "$report" ]] || { printf 'Refusing to overwrite report: %s\n' "$report" >&2; exit 2; }
  [[ -d "$(dirname "$report")" ]] || { printf 'Report parent does not exist: %s\n' "$(dirname "$report")" >&2; exit 2; }
  exec > "$report"
fi

sandbox=$(mktemp -d "/tmp/dirgo-bench.XXXXXX")
owned_fixture=0
fixture_parent=''
cleanup() {
  rm -rf "$sandbox"
  if ((owned_fixture && !keep)); then rm -rf "$fixture_parent"; fi
}
trap cleanup EXIT

if [[ -z "$fixture" ]]; then
  fixture_parent=$(mktemp -d "/tmp/dirgo-fixture-parent.XXXXXX")
  fixture="$fixture_parent/fixture"
  owned_fixture=1
  "$fixture_bin" --output "$fixture" --directories "$directories"
elif [[ ! -d "$fixture" ]]; then
  printf 'Fixture does not exist: %s\n' "$fixture" >&2
  exit 2
fi
case "$fixture" in *$'\n'*|*'"'*) printf '%s\n' 'Fixture path cannot contain a newline or double quote' >&2; exit 2 ;; esac

mkdir -p "$sandbox/config/dirgo"
printf 'schema_version = 1\nroots = ["%s"]\n' "$fixture" > "$sandbox/config/dirgo/config.toml"
export XDG_CONFIG_HOME="$sandbox/config"
export XDG_CACHE_HOME="$sandbox/cache"
export XDG_STATE_HOME="$sandbox/state"

time_command() {
  local label=$1
  shift
  local timing="$sandbox/$label.time"
  local status=0
  if /usr/bin/time -p "$@" >/dev/null 2>"$timing"; then
    status=0
  else
    status=$?
  fi
  # `query` intentionally returns 3 for the no-match warm-path measurement.
  if ((status != 0 && status != 3)); then
    cat "$timing" >&2
    return "$status"
  fi
  local real
  real=$(awk '$1 == "real" { print $2 " s" }' "$timing")
  [[ -n "$real" ]] || real='unavailable'
  printf '%-28s %s\n' "$label:" "$real"
}

printf 'Dirgo external CLI benchmark\n\n'
printf 'timestamp_utc                 %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf 'system                        %s\n' "$(uname -srm)"
printf 'rust                          %s\n' "$(rustc --version)"
printf 'dgo                           %s\n' "$("$dgo_bin" --version)"
printf 'fixture                       %s\n' "$fixture"
printf 'fixture requested directories %s\n' "$directories"
printf 'method                        fresh XDG state; cold refresh once; warm commands after refresh\n\n'
time_command cold_refresh "$dgo_bin" refresh
time_command warm_no_match "$dgo_bin" query unlikely-dirgo-benchmark-token
printf '\n'
"$dgo_bin" bench --query node --samples "$samples"
if ((keep && owned_fixture)); then printf '\nPreserved generated fixture: %s\n' "$fixture"; fi
