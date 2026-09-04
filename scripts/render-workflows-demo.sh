#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dgo_bin="${DGO_BIN:-$repo_root/target/release/dgo}"
output="${1:-$repo_root/docs/assets/dirgo-workflows.gif}"
demo_root="$(mktemp -d /private/tmp/dirgo-workflows-demo.XXXXXX)"
trap 'rm -rf "$demo_root"' EXIT

for tool in jq swiftc ffmpeg ffprobe; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done
[[ -x "$dgo_bin" ]] || { echo "Dirgo binary not found: $dgo_bin" >&2; exit 1; }
CLANG_MODULE_CACHE_PATH="$demo_root/swift-module-cache" \
  SWIFT_MODULECACHE_PATH="$demo_root/swift-module-cache" \
  swiftc "$repo_root/scripts/render-workflow-frame.swift" -o "$demo_root/render-workflow-frame"

export XDG_CONFIG_HOME="$demo_root/config"
export XDG_CACHE_HOME="$demo_root/cache"
export XDG_STATE_HOME="$demo_root/state"
export DGO_DISABLE_UPDATE_CHECK=1
alpha="$demo_root/workspace/alpha"
beta="$demo_root/workspace/beta"
mkdir -p "$alpha" "$beta" "$demo_root/rendered" "$XDG_CONFIG_HOME/dirgo"
printf '[workspace]\n' >"$alpha/Cargo.toml"
printf '{"name":"beta","scripts":{"lint":"eslint .","test":"vitest run","build":"vite build"}}\n' >"$beta/package.json"
printf 'schema_version = 1\nroots = [%s, %s]\n' \
  "$(printf '%s' "$alpha" | jq -Rsa .)" "$(printf '%s' "$beta" | jq -Rsa .)" \
  >"$XDG_CONFIG_HOME/dirgo/config.toml"

"$dgo_bin" suggestions enable >/dev/null
"$dgo_bin" suggestions history enable >/dev/null
"$dgo_bin" workflows enable >/dev/null

record() {
  local command="$1" cwd="$2" session="$3" started="$4"
  jq -cn --arg command "$command" --arg cwd "$cwd" --arg session "$session" \
    --argjson started "$started" \
    '{protocol_version:2,command:$command,cwd:$cwd,exit_code:0,duration_ms:84,session_id:$session,shell:"zsh",started_at:$started}' |
    "$dgo_bin" __suggest-record
}

request() {
  local cwd="$1" before="$2"
  jq -cn --arg cwd "$cwd" --arg before "$before" \
    '{protocol_version:2,request_id:1,shell:"zsh",cwd:$cwd,before_cursor:$before,after_cursor:"",max_results:8,terminal_rows:24,terminal_columns:100,presentation:"list"}' |
    "$dgo_bin" __suggest
}

record 'cargo fmt' "$alpha" alpha-1 1800000001
record 'cargo test' "$alpha" alpha-1 1800000002
export DGO_SESSION_ID=alpha-1
early="$(request "$alpha" 'cargo c')"
[[ "$(jq '[.suggestions[] | select(.source == "workflow")] | length' <<<"$early")" == 0 ]]

timestamp=1800000010
for session in alpha-2 alpha-3; do
  record 'cargo fmt' "$alpha" "$session" "$timestamp"; timestamp=$((timestamp + 1))
  record 'cargo test' "$alpha" "$session" "$timestamp"; timestamp=$((timestamp + 1))
  record 'cargo clippy' "$alpha" "$session" "$timestamp"; timestamp=$((timestamp + 2))
done
for session in beta-1 beta-2 beta-3; do
  record 'npm run lint' "$beta" "$session" "$timestamp"; timestamp=$((timestamp + 1))
  record 'npm run test' "$beta" "$session" "$timestamp"; timestamp=$((timestamp + 1))
  record 'npm run build' "$beta" "$session" "$timestamp"; timestamp=$((timestamp + 2))
done

export DGO_SESSION_ID=active-demo
record 'cargo fmt' "$alpha" "$DGO_SESSION_ID" "$timestamp"; timestamp=$((timestamp + 1))
record 'cargo test' "$alpha" "$DGO_SESSION_ID" "$timestamp"; timestamp=$((timestamp + 1))
record 'cargo clippy' "$alpha" "$DGO_SESSION_ID" "$timestamp"; timestamp=$((timestamp + 1))
(cd "$alpha" && "$dgo_bin" workflows save 'Quality gate' --last 3 --yes) >/dev/null
record 'cargo fmt' "$alpha" "$DGO_SESSION_ID" "$timestamp"

status_json="$("$dgo_bin" workflows status --json)"
alpha_json="$("$dgo_bin" workflows list --project "$alpha" --json)"
beta_json="$("$dgo_bin" workflows list --project "$beta" --json)"
suggestions="$(request "$alpha" 'cargo t')"
palette_json="$("$dgo_bin" __palette-json --cwd "$alpha")"
export_file="$demo_root/workflows.jsonl"
"$dgo_bin" workflows export --project "$alpha" --output "$export_file" >/dev/null

jq -e '.enabled and .schema_version == 3 and .saved_count == 1' <<<"$status_json" >/dev/null
jq -e '.learned | map(select(.next_command == "cargo test")) | length > 0' <<<"$alpha_json" >/dev/null
jq -e '.learned | map(select(.next_command == "npm run test")) | length > 0' <<<"$beta_json" >/dev/null
jq -e '.suggestions | map(select(.source == "workflow" and .edit.replacement == "cargo test")) | length > 0' <<<"$suggestions" >/dev/null
jq -e '.items | map(select(.source == "workflows" and .action.kind == "insert" and .action.text == "cargo test")) | length > 0' <<<"$palette_json" >/dev/null
[[ "$(jq -r '.workflow.scope_key' "$export_file" | sort -u)" == project ]]

xml_escape() {
  local value="$1"
  value="${value//&/&amp;}"; value="${value//</&lt;}"; value="${value//>/&gt;}"
  printf '%s' "$value"
}

scene() {
  local index="$1" badge="$2" title="$3" subtitle="$4"; shift 4
  local svg="$demo_root/scene-$index.svg" y=265 line
  {
    printf '<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="760" viewBox="0 0 1200 760">'
    printf '<rect width="1200" height="760" fill="#07111F"/>'
    printf '<text x="60" y="62" fill="#30D158" font-family="Menlo,monospace" font-size="15" font-weight="700">DIRGO 0.8 · WORKFLOW INTELLIGENCE</text>'
    printf '<text x="60" y="115" fill="#F2F7FA" font-family="-apple-system,BlinkMacSystemFont,Arial,sans-serif" font-size="34" font-weight="700">%s</text>' "$(xml_escape "$title")"
    printf '<text x="60" y="148" fill="#90A4B7" font-family="-apple-system,BlinkMacSystemFont,Arial,sans-serif" font-size="18">%s</text>' "$(xml_escape "$subtitle")"
    printf '<rect x="52" y="182" width="1096" height="430" rx="24" fill="#0D1B2A" stroke="#20364D"/>'
    printf '<circle cx="82" cy="212" r="6" fill="#FF5F57"/><circle cx="102" cy="212" r="6" fill="#FEBC2E"/><circle cx="122" cy="212" r="6" fill="#28C840"/>'
    for line in "$@"; do
      printf '<text x="88" y="%s" fill="#D7E1EA" font-family="Menlo,monospace" font-size="19">%s</text>' "$y" "$(xml_escape "$line")"
      y=$((y + 48))
    done
    printf '<text x="88" y="580" fill="#30D158" font-family="Menlo,monospace" font-size="14" font-weight="700">%s</text>' "$(xml_escape "$badge")"
    printf '<rect x="52" y="640" width="1096" height="4" rx="2" fill="#20364D"/><rect x="52" y="640" width="%s" height="4" rx="2" fill="#30D158"/>' "$((index * 137))"
    printf '<text x="1050" y="680" fill="#74889B" font-family="Menlo,monospace" font-size="14">%s / 8</text>' "$index"
    printf '</svg>'
  } >"$svg"
  "$demo_root/render-workflow-frame" "$index" "$demo_root/rendered/scene-$index.png" \
    "$badge" "$title" "$subtitle" "$@"
}

scene 1 'LOCAL · EXPLAINABLE · USER CONTROLLED' 'Your next command, from local context' \
  'Exact evidence from completed commands. No cloud. No runner.' \
  '$ dgo suggestions history enable' '$ dgo workflows enable' \
  'Workflow suggestions enabled. Commands are suggested, never executed.'
scene 2 'PROJECT ISOLATION' 'Two projects learn different sequences' \
  'Canonical roots keep identical habits from leaking across workspaces.' \
  'alpha  · cargo fmt → cargo test → cargo clippy' \
  'beta   · npm run lint → npm run test → npm run build'
scene 3 'WEAK EVIDENCE SUPPRESSED' 'Learning, without guessing early' \
  'One session is never enough evidence for a learned next action.' \
  '$ cargo c▌' 'No NEXT result after the first session.' \
  'Threshold: 3 observations · 2 distinct sessions'
scene 4 'NEXT' 'Repeated evidence becomes useful' \
  'Saved choices lead; learned choices remain deterministic and inspectable.' \
  '$ cargo t▌' 'NEXT  cargo test' \
  'Next in this project · repeated successful evidence'
scene 5 'WORKSPACE PALETTE' 'Preview the sequence before insertion' \
  'Tasks  Workflows  Git' \
  'Quality gate · Saved · 3 steps' \
  '  1. cargo fmt' '> 2. cargo test  NEXT' '  3. cargo clippy'
scene 6 'INSERTED, NEVER EXECUTED' 'One selected step enters the buffer' \
  'There is no hidden queue and no synthesized Enter key.' \
  '$ cargo t▌' 'Tab  →  cargo test▌' 'Enter remains yours.'
scene 7 'PRIVATE EXPORT' 'Inspect and move your own data safely' \
  'Scoped management never needs command text in process arguments.' \
  '$ dgo workflows list --project .' \
  '$ dgo workflows export --project . --output workflows.jsonl' \
  'schema v3 · project path redacted · mode 0600'
scene 8 'READY FOR YOUR SHELL' 'Local, explainable, under user control' \
  'Zsh · Bash 4+ · Fish · PowerShell 7+' \
  'Bounded evidence · deterministic ranking · one-step insertion' \
  'dgo workflows disable  stops ranking without deleting your data'

if grep -R -E '/[U]sers/|/private/|dirgo-workflows-demo\.' "$demo_root"/scene-*.svg >/dev/null; then
  echo 'rendered scene contains a personal or temporary path' >&2
  exit 1
fi

concat_file="$demo_root/scenes.txt"
for index in 1 2 3 4 5 6 7 8; do
  printf "file '%s'\nduration 2.7\n" "$demo_root/rendered/scene-$index.png" >>"$concat_file"
done
printf "file '%s'\n" "$demo_root/rendered/scene-8.png" >>"$concat_file"
video="$demo_root/workflows.mp4"
palette="$demo_root/colors.png"
ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$concat_file" -vf 'fps=18,scale=1100:-2:flags=lanczos,format=yuv420p' -c:v libx264 -crf 18 "$video"
ffmpeg -hide_banner -loglevel error -y -i "$video" -vf 'fps=15,palettegen=max_colors=112:stats_mode=diff' "$palette"
ffmpeg -hide_banner -loglevel error -y -i "$video" -i "$palette" -filter_complex '[0:v]fps=15[video];[video][1:v]paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle' -loop 0 "$output"

width="$(ffprobe -v error -select_streams v:0 -show_entries stream=width -of csv=p=0 "$output")"
height="$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of csv=p=0 "$output")"
duration="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$output")"
frames="$(ffprobe -v error -select_streams v:0 -count_frames -show_entries stream=nb_read_frames -of csv=p=0 "$output")"
[[ "$width" == 1100 && "$height" == 696 ]] || { echo "unexpected GIF dimensions: ${width}x${height}" >&2; exit 1; }
awk -v duration="$duration" 'BEGIN { exit !(duration >= 23 && duration <= 26) }'
[[ "$frames" -ge 340 ]] || { echo "GIF frame count is too small: $frames" >&2; exit 1; }
printf 'WORKFLOWS-GIF:ok width=%s height=%s duration=%ss frames=%s bytes=%s\n' \
  "$width" "$height" "$duration" "$frames" "$(wc -c <"$output" | tr -d ' ')"
