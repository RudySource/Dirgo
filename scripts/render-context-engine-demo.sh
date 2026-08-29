#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dgo_bin="${DGO_BIN:-$repo_root/target/release/dgo}"
output="${1:-$repo_root/docs/assets/dirgo-context-engine.gif}"
demo_root="$(mktemp -d /private/tmp/dirgo-context-demo.XXXXXX)"
trap 'rm -rf "$demo_root"' EXIT

for tool in jq qlmanage ffmpeg ffprobe; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done
[[ -x "$dgo_bin" ]] || { echo "Dirgo binary not found: $dgo_bin" >&2; exit 1; }

export XDG_CONFIG_HOME="$demo_root/config"
export XDG_CACHE_HOME="$demo_root/cache"
export XDG_STATE_HOME="$demo_root/state"
export DGO_DISABLE_UPDATE_CHECK=1
export DGO_SESSION_ID=demo-session
alpha="$demo_root/workspace/alpha"
beta="$demo_root/workspace/beta"
mkdir -p "$alpha" "$beta" "$demo_root/rendered"
printf '[workspace]\n' >"$alpha/Cargo.toml"
printf '[workspace]\n' >"$beta/Cargo.toml"

"$dgo_bin" suggestions enable >/dev/null
"$dgo_bin" suggestions history enable >/dev/null

record() {
  local command="$1" cwd="$2" exit_code="$3" started="$4"
  jq -cn --arg command "$command" --arg cwd "$cwd" --arg session "$DGO_SESSION_ID" \
    --argjson exit_code "$exit_code" --argjson started "$started" \
    '{protocol_version:2,command:$command,cwd:$cwd,exit_code:$exit_code,duration_ms:84,session_id:$session,shell:"zsh",started_at:$started}' |
    "$dgo_bin" __suggest-record
}

record 'cargo test --workspace' "$alpha" 0 1800000001
record 'cargo test --workspace' "$alpha" 0 1800000002
record 'cargo test --workspace' "$alpha" 0 1800000003
record 'cargo test --doc' "$alpha" 0 1800000004
record 'cargo test --all-targets' "$alpha" 0 1800000005
record 'cargo test --workspace' "$beta" 1 1800000006
record 'cargo test --workspace' "$beta" 1 1800000007
record 'cargo test --workspace' "$beta" 1 1800000008
record 'cargo test --workspace' "$beta" 1 1800000009

status_json="$("$dgo_bin" suggestions history status --json)"
alpha_json="$("$dgo_bin" suggestions history list --project "$alpha" --json)"
beta_json="$("$dgo_bin" suggestions history list --project "$beta" --json)"
request="$(jq -cn --arg cwd "$alpha" '{protocol_version:2,request_id:1,shell:"zsh",cwd:$cwd,before_cursor:"cargo test --",after_cursor:"",max_results:8,terminal_rows:24,terminal_columns:100,presentation:"list"}')"
suggestions="$(printf '%s\n' "$request" | "$dgo_bin" __suggest)"
export_file="$demo_root/history.jsonl"
"$dgo_bin" suggestions history export --project "$alpha" --output "$export_file" >/dev/null
inserted="$(printf '%s\0%s\0' 'cargo test --w' '' | "$dgo_bin" __suggest-shell --shell zsh --cwd "$alpha")"

[[ "$(jq -r '.event | has("cwd") or has("project_root")' "$export_file" | sort -u)" == "false" ]]
[[ "$inserted" == 'cargo test --workspace' ]]
[[ "$(jq '[.[] | select(.command == "cargo test --workspace")][0].success_count' <<<"$alpha_json")" == "3" ]]
[[ "$(jq '[.[] | select(.command == "cargo test --workspace")][0].failure_count' <<<"$beta_json")" == "4" ]]
alpha_1="$(jq -r '.[0] | "✓  \(.command)   used \(.use_count)x   success \(.success_count)"' <<<"$alpha_json")"
alpha_2="$(jq -r '.[1] | "✓  \(.command)   used \(.use_count)x   success \(.success_count)"' <<<"$alpha_json")"
alpha_3="$(jq -r '.[2] | "✓  \(.command)   used \(.use_count)x   success \(.success_count)"' <<<"$alpha_json")"
suggestion_1="$(jq -r '.suggestions[0] | "›  \(.display)   [\(.description)]"' <<<"$suggestions")"
suggestion_2="$(jq -r '.suggestions[1] | "   \(.display)   [\(.description)]"' <<<"$suggestions")"
suggestion_3="$(jq -r '.suggestions[2] | "   \(.display)   [\(.description)]"' <<<"$suggestions")"

xml_escape() {
  local value="$1"
  value="${value//&/&amp;}"; value="${value//</&lt;}"; value="${value//>/&gt;}"
  printf '%s' "$value"
}

scene() {
  local index="$1" badge="$2" title="$3"; shift 3
  local svg="$demo_root/scene-$index.svg" y=246 line
  {
    printf '<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="1200" viewBox="0 0 1200 1200">'
    printf '<rect width="1200" height="1200" fill="#07111F"/>'
    printf '<rect x="54" y="46" width="1092" height="535" rx="28" fill="#0D1B2A" stroke="#20364D"/>'
    printf '<text x="92" y="100" fill="#64D2FF" font-family="Menlo,monospace" font-size="16" font-weight="700">DIRGO 0.6 · CONTEXT ENGINE</text>'
    printf '<text x="92" y="155" fill="#EAF2F8" font-family="Menlo,monospace" font-size="30" font-weight="700">%s</text>' "$(xml_escape "$title")"
    printf '<rect x="92" y="183" width="1016" height="315" rx="16" fill="#050B13" stroke="#20364D"/>'
    printf '<circle cx="120" cy="212" r="6" fill="#FF5F57"/><circle cx="140" cy="212" r="6" fill="#FEBC2E"/><circle cx="160" cy="212" r="6" fill="#28C840"/>'
    for line in "$@"; do
      printf '<text x="120" y="%s" fill="#C9D6E2" font-family="Menlo,monospace" font-size="19">%s</text>' "$y" "$(xml_escape "$line")"
      y=$((y + 38))
    done
    printf '<rect x="92" y="525" width="1016" height="3" rx="2" fill="#20364D"/><rect x="92" y="525" width="%s" height="3" rx="2" fill="#30D158"/>' "$((index * 169))"
    printf '<text x="1040" y="557" fill="#8FA4B8" font-family="Menlo,monospace" font-size="14">%s / 6</text>' "$index"
    printf '<text x="92" y="557" fill="#30D158" font-family="Menlo,monospace" font-size="14" font-weight="700">%s</text>' "$(xml_escape "$badge")"
    printf '</svg>'
  } >"$svg"
  qlmanage -t -s 1200 -o "$demo_root/rendered" "$svg" >/dev/null
}

scene 1 'OPT-IN' 'Local context is explicitly opt-in' \
  '$ dgo suggestions history enable' \
  'Filtered local command history enabled.' \
  "$ $(jq -r '"schema v\(.schema_version)  ·  \(.event_count) events  ·  \(.aggregate_count) aggregates"' <<<"$status_json")"
scene 2 'PROJECT α' 'The same command has project-local memory' \
  '$ dgo suggestions history list --project alpha' \
  "$alpha_1" "$alpha_2" "$alpha_3"
scene 3 'OUTCOME' 'Failures stay inside their project' \
  '$ dgo suggestions history list --project beta' \
  "$(jq -r '.[] | select(.command == "cargo test --workspace") | "×  \(.command)   used \(.use_count)x   failures \(.failure_count)"' <<<"$beta_json")" \
  'Unknown legacy outcomes remain neutral.'
scene 4 'RANKING' 'Real aggregates rank useful choices' \
  '$ cargo test --' \
  "$suggestion_1" "$suggestion_2" "$suggestion_3" \
  'Current project + cwd + successful outcomes win.'
scene 5 'PRIVACY' 'Export is inspectable and path-redacted' \
  '$ dgo suggestions history export --project alpha --output history.jsonl' \
  "$(wc -l <"$export_file" | tr -d ' ') versioned JSONL events exported" \
  'cwd key: absent   ·   project_root key: absent' \
  'Existing files need --force. Symlinks are refused.'
scene 6 'INSERT ONLY' 'Inserted as text. Nothing executed.' \
  '$ cargo test --w▌' \
  "Tab  →  ${inserted}▌" \
  'Cursor stays in the buffer. Enter remains yours.'

concat_file="$demo_root/scenes.txt"
for index in 1 2 3 4 5 6; do
  printf "file '%s'\nduration %s\n" "$demo_root/rendered/scene-$index.svg.png" "$([[ $index == 1 || $index == 6 ]] && echo 3.4 || echo 3.8)" >>"$concat_file"
done
printf "file '%s'\n" "$demo_root/rendered/scene-6.svg.png" >>"$concat_file"
video="$demo_root/context.mp4"
palette="$demo_root/palette.png"
ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$concat_file" -vf 'crop=1200:675:0:0,fps=18,scale=1100:-2:flags=lanczos,format=yuv420p' -c:v libx264 -crf 18 "$video"
ffmpeg -hide_banner -loglevel error -y -i "$video" -vf 'fps=15,palettegen=max_colors=96:stats_mode=diff' "$palette"
ffmpeg -hide_banner -loglevel error -y -i "$video" -i "$palette" -filter_complex '[0:v]fps=15[video];[video][1:v]paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle' -loop 0 "$output"
ffprobe -v error -show_entries format=duration,size -show_entries stream=width,height,nb_frames -of default=noprint_wrappers=1 "$output"
