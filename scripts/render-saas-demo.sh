#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_gif="${1:-$repo_root/docs/assets/dirgo-project-commands.gif}"
output_gif="${2:-$repo_root/docs/assets/dirgo-project-commands-saas.gif}"
scene_dir="$repo_root/docs/demo/saas"

ffmpeg_bin="${FFMPEG_BIN:-$(command -v ffmpeg || true)}"
ffprobe_bin="${FFPROBE_BIN:-$(command -v ffprobe || true)}"

if [[ -z "$ffmpeg_bin" || -z "$ffprobe_bin" ]]; then
  echo "ffmpeg and ffprobe are required" >&2
  exit 1
fi

if [[ ! -f "$source_gif" ]]; then
  echo "source GIF not found: $source_gif" >&2
  exit 1
fi

for scene in intro frame header-project header-git header-cargo outro; do
  if [[ ! -f "$scene_dir/$scene.svg" ]]; then
    echo "scene source not found: $scene_dir/$scene.svg" >&2
    exit 1
  fi
done

scratch_dir="$(mktemp -d "${TMPDIR:-/tmp}/dirgo-saas-demo.XXXXXX")"
trap 'rm -rf "$scratch_dir"' EXIT

render_svg() {
  local source_svg="$1"
  local output_png="$2"
  local scene_name
  local crop_height="675"
  scene_name="$(basename "$source_svg")"

  if [[ "$scene_name" == header-* ]]; then
    crop_height="105"
  fi

  if command -v rsvg-convert >/dev/null 2>&1; then
    rsvg-convert -w 1200 -h 1200 "$source_svg" -o "$scratch_dir/$scene_name.png"
  elif command -v qlmanage >/dev/null 2>&1; then
    qlmanage -t -s 1200 -o "$scratch_dir" "$source_svg" >/dev/null
  else
    echo "rsvg-convert or macOS qlmanage is required to render SVG scenes" >&2
    exit 1
  fi

  "$ffmpeg_bin" -hide_banner -loglevel error -y \
    -i "$scratch_dir/$scene_name.png" -vf "crop=1200:${crop_height}:0:0" -frames:v 1 "$output_png"
}

for scene in intro frame header-project header-git header-cargo outro; do
  render_svg "$scene_dir/$scene.svg" "$scratch_dir/$scene.png"
done

presentation_mp4="$scratch_dir/presentation.mp4"
palette_png="$scratch_dir/palette.png"

"$ffmpeg_bin" -hide_banner -loglevel error -y \
  -i "$source_gif" \
  -loop 1 -framerate 20 -i "$scratch_dir/intro.png" \
  -loop 1 -framerate 20 -i "$scratch_dir/frame.png" \
  -loop 1 -framerate 20 -i "$scratch_dir/header-project.png" \
  -loop 1 -framerate 20 -i "$scratch_dir/header-git.png" \
  -loop 1 -framerate 20 -i "$scratch_dir/header-cargo.png" \
  -loop 1 -framerate 20 -i "$scratch_dir/outro.png" \
  -filter_complex "\
    [0:v]trim=start=0:end=18.8,setpts=PTS-STARTPTS,fps=20,scale=986:554:flags=lanczos,zoompan=z='min(zoom+0.000095,1.035)':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d=1:s=986x554:fps=20,format=rgba[terminal];\
    [1:v]scale=1200:675,format=rgba,split=2[intro_hold_source][intro_zoom_source];\
    [intro_hold_source]trim=duration=2.4,setpts=PTS-STARTPTS[intro_hold];\
    [intro_zoom_source]trim=duration=0.8,setpts=PTS-STARTPTS,zoompan=z='min(1+0.035*on,1.55)':x='min(max(0,820-(iw/zoom/2)),iw-iw/zoom)':y='min(max(0,300-(ih/zoom/2)),ih-ih/zoom)':d=1:s=1200x675:fps=20,format=rgba[intro_zoom];\
    [intro_hold][intro_zoom]concat=n=2:v=1:a=0,fps=20,settb=AVTB[intro];\
    [2:v]trim=duration=18.8,setpts=PTS-STARTPTS,scale=1200:675,format=rgba[stage];\
    [stage][terminal]overlay=x=107:y=112:shortest=1[main0];\
    [3:v]trim=duration=18.8,setpts=PTS-STARTPTS,scale=1200:105,format=rgba[project];\
    [4:v]trim=duration=18.8,setpts=PTS-STARTPTS,scale=1200:105,format=rgba[git];\
    [5:v]trim=duration=18.8,setpts=PTS-STARTPTS,scale=1200:105,format=rgba[cargo];\
    [main0][project]overlay=0:0:enable='between(t,0,6.45)'[main1];\
    [main1][git]overlay=0:0:enable='between(t,6.45,12.75)'[main2];\
    [main2][cargo]overlay=0:0:enable='between(t,12.75,18.8)',fps=20,settb=AVTB[main];\
    [6:v]trim=duration=3.2,setpts=PTS-STARTPTS,scale=1200:675,format=rgba,fps=20,settb=AVTB[outro];\
    [intro][main]xfade=transition=fade:duration=0.25:offset=2.95[first];\
    [first][outro]xfade=transition=fade:duration=0.45:offset=21.30,format=yuv420p,pad=1200:676:0:0:color=#050A12[presentation]" \
  -map "[presentation]" -an -r 20 -c:v libx264 -preset medium -crf 18 -movflags +faststart "$presentation_mp4"

"$ffmpeg_bin" -hide_banner -loglevel error -y -i "$presentation_mp4" \
  -vf "fps=18,scale=1100:-1:flags=lanczos,palettegen=max_colors=112:stats_mode=diff" \
  "$palette_png"

"$ffmpeg_bin" -hide_banner -loglevel error -y -i "$presentation_mp4" -i "$palette_png" \
  -filter_complex "[0:v]fps=18,scale=1100:-1:flags=lanczos[video];[video][1:v]paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle" \
  -loop 0 "$output_gif"

"$ffprobe_bin" -v error \
  -show_entries format=duration,size \
  -show_entries stream=width,height,avg_frame_rate,nb_frames \
  -of default=noprint_wrappers=1 "$output_gif"
