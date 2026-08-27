# Dirgo product video

Deterministic, code-built product animation for Dirgo 0.5. Both compositions
render at 1920×1080, 60 FPS, with no screen recording, runtime data, audio, or
randomness.

## Compositions

| ID | Duration | Purpose |
| --- | ---: | --- |
| `DirgoHero` | 20.5 s | Primary product story: navigation, Git suggestions, project commands, and final brand frame. |
| `DirgoLoop` | 8 s | Seamless website/social loop focused on safe Git command insertion. |

The behavior and copy are documented in [CONCEPT.md](./CONCEPT.md).

## Requirements

- Node.js 20 or newer
- npm
- A local Chromium download on the first Remotion render

## Preview

```bash
cd marketing/video
npm install
npm run dev
```

Remotion Studio opens both compositions with frame-accurate scrubbing.

## Verify

```bash
npm run test
npm run typecheck
npm run lint
npm run build
```

## Render

Render every deliverable:

```bash
npm run render
```

Or render one composition/codec:

```bash
npm run render:hero
npm run render:hero:webm
npm run render:loop
npm run render:loop:webm
```

Outputs are written to `marketing/video/out/` and deliberately ignored by Git.
MP4 uses H.264 with 4:2:0 color for broad compatibility; WebM uses VP9 for web
delivery.

## Structure

```text
src/
  animations/   deterministic motion primitives
  components/   shared canvas, terminal, prompt, results, and brand UI
  content/      verified demo candidates and descriptions
  scenes/       reusable story scenes
  timeline/     frame timing and deterministic typing
```

The marketing package is excluded from the published Rust crate and does not
change or execute the Dirgo CLI.
