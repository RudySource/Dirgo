#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dgo_bin="${DGO_BIN:-$repo_root/target/release/dgo}"
output="${1:-$repo_root/docs/assets/dirgo-workspace-palette.gif}"
demo_root="$(mktemp -d /private/tmp/dirgo-workspace-palette.XXXXXX)"
trap 'rm -rf "$demo_root"' EXIT

for tool in jq qlmanage ffmpeg ffprobe git; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done
[[ -x "$dgo_bin" ]] || { echo "Dirgo binary not found: $dgo_bin" >&2; exit 1; }

export XDG_CONFIG_HOME="$demo_root/config"
export XDG_CACHE_HOME="$demo_root/cache"
export XDG_STATE_HOME="$demo_root/state"
export DGO_DISABLE_UPDATE_CHECK=1
project="$demo_root/workspace/Atlas"
focused="$demo_root/system/Library/Application Support/Adobe/CEP/extensions"
mkdir -p "$project/src" "$project/.github" "$focused" "$demo_root/rendered" "$XDG_CONFIG_HOME/dirgo"
printf 'fn main() { println!("workspace palette"); }\n' >"$project/src/main.rs"
printf '{"name":"atlas","scripts":{"dev":"vite","test:unit":"vitest run","lint":"eslint ."}}\n' >"$project/package.json"
printf 'services:\n  api:\n    image: example.invalid/api\n  worker:\n    image: example.invalid/worker\n' >"$project/compose.yaml"
printf 'schema_version = 1\nroots = [%s, %s]\n' \
  "$(printf '%s' "$project" | jq -Rsa .)" "$(printf '%s' "$focused" | jq -Rsa .)" \
  >"$XDG_CONFIG_HOME/dirgo/config.toml"

git -C "$project" init -q
git -C "$project" config user.name 'Dirgo Demo'
git -C "$project" config user.email 'demo@example.invalid'
git -C "$project" add package.json compose.yaml src/main.rs
git -C "$project" commit -qm 'demo baseline'
git -C "$project" branch feature/palette

"$dgo_bin" refresh >/dev/null
"$dgo_bin" bookmark add studio --path "$project" >/dev/null
palette_json="$demo_root/palette.json"
"$dgo_bin" __palette-json --cwd "$project" >"$palette_json"
roots_json="$demo_root/roots.json"
"$dgo_bin" roots list --json >"$roots_json"
path_json="$demo_root/path.json"
"$dgo_bin" explain 'library/adobe/cep/extensions' >"$path_json"

jq -e '
  ([.items[].source] | index("files")) != null and
  ([.items[].source] | index("tasks")) != null and
  ([.items[].source] | index("git")) != null and
  ([.items[].source] | index("compose")) != null and
  ([.items[].source] | index("places")) != null
' "$palette_json" >/dev/null
jq -e 'map(select(.focused == true and .accessible == true)) | length == 1' "$roots_json" >/dev/null
jq -e '.candidates | map(.basename) | index("extensions") != null' "$path_json" >/dev/null

file_item="$(jq -r '[.items[] | select(.source == "files" and .title == "src/main.rs")][0].title' "$palette_json")"
task_item="$(jq -r '[.items[] | select(.source == "tasks" and .title == "dev")][0].title' "$palette_json")"
branch_item="$(jq -r '[.items[] | select(.source == "git" and .title == "feature/palette")][0].title' "$palette_json")"
compose_item="$(jq -r '[.items[] | select(.source == "compose" and .title == "api")][0].title' "$palette_json")"
place_item="$(jq -r '[.items[] | select(.source == "places" and .title == "@studio")][0].title' "$palette_json")"
for value in "$file_item" "$task_item" "$branch_item" "$compose_item" "$place_item"; do
  [[ "$value" != null && -n "$value" ]] || { echo "real palette fixture is incomplete" >&2; exit 1; }
done

xml_escape() {
  local value="$1"
  value="${value//&/&amp;}"
  value="${value//</&lt;}"
  value="${value//>/&gt;}"
  printf '%s' "$value"
}

scene() {
  local index="$1" badge="$2" title="$3" subtitle="$4" selected_source="$5"
  shift 5
  local svg="$demo_root/scene-$index.svg" y=286 line marker text
  {
    printf '<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="1200" viewBox="0 0 1200 1200">'
    printf '<rect width="1200" height="1200" fill="#07111F"/>'
    printf '<text x="56" y="58" fill="#30D158" font-family="Menlo,monospace" font-size="15" font-weight="700" letter-spacing="1.2">DIRGO 0.7</text>'
    printf '<text x="56" y="105" fill="#F2F7FA" font-family="-apple-system,BlinkMacSystemFont,Arial,sans-serif" font-size="32" font-weight="700">%s</text>' "$(xml_escape "$title")"
    printf '<text x="56" y="134" fill="#90A4B7" font-family="-apple-system,BlinkMacSystemFont,Arial,sans-serif" font-size="17">%s</text>' "$(xml_escape "$subtitle")"
    printf '<rect x="48" y="166" width="1104" height="455" rx="24" fill="#0D1B2A" stroke="#20364D"/>'
    printf '<circle cx="78" cy="196" r="6" fill="#FF5F57"/><circle cx="98" cy="196" r="6" fill="#FEBC2E"/><circle cx="118" cy="196" r="6" fill="#28C840"/>'
    printf '<text x="148" y="202" fill="#90A4B7" font-family="Menlo,monospace" font-size="14">Workspace Palette</text>'
    local x=72 source
    for source in All Files Tasks Git Compose Places; do
      local width=$(( ${#source} * 10 + 28 ))
      if [[ "$source" == "$selected_source" ]]; then
        printf '<rect x="%s" y="220" width="%s" height="31" rx="10" fill="#123623" stroke="#30D158"/>' "$x" "$width"
        printf '<text x="%s" y="241" fill="#8AE2B5" font-family="Menlo,monospace" font-size="13" font-weight="700">%s</text>' "$((x + 14))" "$source"
      else
        printf '<text x="%s" y="241" fill="#74889B" font-family="Menlo,monospace" font-size="13">%s</text>' "$((x + 14))" "$source"
      fi
      x=$((x + width + 12))
    done
    for line in "$@"; do
      marker="${line%%|*}"
      text="${line#*|}"
      if [[ "$marker" == ">" ]]; then
        printf '<rect x="72" y="%s" width="690" height="42" rx="11" fill="#10291D"/>' "$((y - 27))"
        printf '<text x="90" y="%s" fill="#30D158" font-family="Menlo,monospace" font-size="17" font-weight="700">›</text>' "$y"
        printf '<text x="119" y="%s" fill="#F2F7FA" font-family="Menlo,monospace" font-size="17" font-weight="700">%s</text>' "$y" "$(xml_escape "$text")"
      elif [[ "$marker" == "+" ]]; then
        printf '<text x="90" y="%s" fill="#8AE2B5" font-family="Menlo,monospace" font-size="16">✓</text>' "$y"
        printf '<text x="119" y="%s" fill="#C9D6E2" font-family="Menlo,monospace" font-size="16">%s</text>' "$y" "$(xml_escape "$text")"
      else
        printf '<text x="119" y="%s" fill="#90A4B7" font-family="Menlo,monospace" font-size="16">%s</text>' "$y" "$(xml_escape "$text")"
      fi
      y=$((y + 48))
    done
    printf '<line x1="790" y1="270" x2="790" y2="560" stroke="#20364D"/>'
    printf '<text x="822" y="300" fill="#74889B" font-family="Menlo,monospace" font-size="13" font-weight="700">PREVIEW</text>'
    printf '<text x="822" y="340" fill="#F2F7FA" font-family="Menlo,monospace" font-size="17">%s</text>' "$(xml_escape "$badge")"
    printf '<text x="822" y="382" fill="#90A4B7" font-family="Menlo,monospace" font-size="14">Lazy · bounded · local</text>'
    printf '<text x="72" y="590" fill="#74889B" font-family="Menlo,monospace" font-size="13">Alt+P open   Tab source   ↑↓ move   Enter select   Esc close</text>'
    printf '<rect x="48" y="642" width="1104" height="3" rx="2" fill="#20364D"/><rect x="48" y="642" width="%s" height="3" rx="2" fill="#30D158"/>' "$((index * 184))"
    printf '</svg>'
  } >"$svg"
  qlmanage -t -s 1200 -o "$demo_root/rendered" "$svg" >/dev/null
}

scene 1 'One bounded snapshot' 'Everything your workspace can offer' 'Files, commands, services, Git and places — one shortcut.' All \
  ">|$file_item    FILE    Rust source" \
  " |$task_item             TASK    package.json script" \
  " |$branch_item  GIT     branch" \
  " |$compose_item             COMPOSE service" \
  " |$place_item         PLACE   bookmark"

scene 2 'No provider restart' 'Switch sources without losing the query' 'Tab changes the lens; the in-memory snapshot stays put.' Tasks \
  ">|dev              package.json · atlas" \
  " |test:unit        package.json · atlas" \
  " |lint             package.json · atlas" \
  "+|Query preserved:  test" \
  "+|Providers were collected once"

scene 3 '90 ms debounce' 'Preview only when you settle' 'Fast navigation stays fast; stale preview work is discarded.' Files \
  ">|src/main.rs      FILE" \
  " |src              DIRECTORY" \
  " |package.json     FILE" \
  "+|Preview: src/main.rs" \
  "+|No file-content search"

scene 4 'Inserted, not executed' 'Commands stay under your control' 'Tasks, Compose and branches enter the editor buffer only.' Git \
  ">|git switch -- feature/palette" \
  " |npm run dev" \
  " |docker compose up api" \
  "+|Literal arguments · no eval" \
  "+|Enter remains yours"

scene 5 'Focused root' 'Open anywhere without indexing everything' 'Add only the ignored system folder you actually need.' Places \
  ">|dgo roots add ~/Library/.../CEP" \
  "+|Focused root · accessible" \
  " |dgo library/adobe/cep/extensions" \
  "+|extensions found through ordered segments" \
  " |No global Library crawl"

scene 6 'Local cache only' 'Know when an update is ready' 'dgo --version never waits for the network.' All \
  ">|●  Update available" \
  " |0.7.0  →  0.8.0" \
  "+|Run dgo --update" \
  " |dgo update-notifications off" \
  "+|Piped version stays one line"

if grep -R -E '/Users/|dirgo-workspace-palette\.[A-Za-z0-9]+' "$demo_root"/scene-*.svg >/dev/null; then
  echo 'rendered scene contains a personal or temporary path' >&2
  exit 1
fi

concat_file="$demo_root/scenes.txt"
for index in 1 2 3 4 5 6; do
  printf "file '%s'\nduration 2.8\n" "$demo_root/rendered/scene-$index.svg.png" >>"$concat_file"
done
printf "file '%s'\n" "$demo_root/rendered/scene-6.svg.png" >>"$concat_file"
video="$demo_root/palette.mp4"
palette="$demo_root/colors.png"
ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$concat_file" -vf 'crop=1200:675:0:0,fps=18,scale=1100:-2:flags=lanczos,format=yuv420p' -c:v libx264 -crf 18 "$video"
ffmpeg -hide_banner -loglevel error -y -i "$video" -vf 'fps=15,palettegen=max_colors=112:stats_mode=diff' "$palette"
ffmpeg -hide_banner -loglevel error -y -i "$video" -i "$palette" -filter_complex '[0:v]fps=15[video];[video][1:v]paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle' -loop 0 "$output"

width="$(ffprobe -v error -select_streams v:0 -show_entries stream=width -of csv=p=0 "$output")"
height="$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of csv=p=0 "$output")"
duration="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$output")"
frames="$(ffprobe -v error -select_streams v:0 -count_frames -show_entries stream=nb_read_frames -of csv=p=0 "$output")"
[[ "$width" == 1100 && "$height" == 618 ]] || { echo "unexpected GIF dimensions: ${width}x${height}" >&2; exit 1; }
awk -v duration="$duration" 'BEGIN { exit !(duration >= 19 && duration <= 21) }'
[[ "$frames" -ge 280 ]] || { echo "GIF frame count is too small: $frames" >&2; exit 1; }
printf 'WORKSPACE-PALETTE-GIF:ok width=%s height=%s duration=%ss frames=%s bytes=%s\n' \
  "$width" "$height" "$duration" "$frames" "$(wc -c <"$output" | tr -d ' ')"
