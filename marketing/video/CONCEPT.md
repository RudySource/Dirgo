# Dirgo premium product animation

## Overall concept

The film follows one developer from finding a project to choosing the next
command. Dirgo is presented as a single continuous layer inside the terminal:
it shortens navigation, makes ambiguous choices visible, and offers safe,
project-aware command insertion without taking control away from the user.

The visual direction is restrained and product-specific. A near-black canvas,
precise typography, and one large terminal object carry the film. A thin green
focus line connects typed input to the active result. It is the only recurring
accent and represents Dirgo narrowing a large local context to one useful next
action.

## Audience and message

- **Audience:** developers who work across several repositories and terminal tools.
- **Immediate job:** reach the right project and continue working without recalling full paths or command syntax.
- **Message:** Dirgo makes the next location and command visible while the shell and the user retain control.

## Verified capabilities selected

### 1. Indexed directory discovery and conservative selection

Dirgo indexes configured roots, supports fuzzy directory queries, and opens an
interactive picker when a result is ambiguous. The picker filters while the
user types and Enter selects the highlighted directory. This is the clearest
demonstration of the original product promise: a fragment replaces a full path.

### 2. Command catalog with descriptions

Opt-in shell suggestions include a compiled command tree for Git and other
common tools. `git co` can offer `commit` and `checkout` with descriptions.
Tab inserts the active suggestion; it never submits the command line.

### 3. Commands declared by the current project

Dirgo 0.5 reads bounded static project manifests in the background and ranks
their commands as `PROJ`. The film uses real `package.json` behavior from the
repository demo fixture: `pnpm run build`, `pnpm run dev`, `pnpm run format`,
`pnpm run lint`, `pnpm run preview`, and `pnpm run test`. Script bodies are not
shown or executed.

## Excluded capabilities

- Bookmarks, recents, back/forward navigation, action flags, setup, doctor,
  updates, configuration, cache internals, and benchmarks are implemented but
  excluded because they weaken the short product story.
- Command-history suggestions are excluded because they require a separate
  explicit opt-in and are less legible than the deterministic command catalog.
- Performance numbers are excluded because local measurements are not universal
  guarantees.
- PowerShell-specific presentation is excluded from the primary film; the
  product behavior is represented with the verified Zsh interaction model.
- No unimplemented feature is represented.

## Primary timeline — `DirgoHero`

Resolution is 1920×1080 at 60 FPS. Duration is 20.5 seconds (1,230 frames).
Transitions overlap so the sequence reads as one camera move rather than a
series of slides.

| Time | Frames | Scene | Content and motion |
| --- | ---: | --- | --- |
| 0.0–2.4 s | 0–143 | Intro | A small Dirgo label and the product text **“Stop remembering paths.”** resolve from blur with a restrained camera settle. |
| 2.0–7.0 s | 120–419 | Find | The text recedes while the terminal emerges from depth. Prompt: `~/dev ❯ dgo sl`. A ranked picker shows `slash`, `slash-api`, `slash-web`, and `slash-docs`; the active result is `slash`. Enter highlights the explicit open action before the scene recedes. |
| 6.5–8.6 s | 390–515 | Control text | Terminal softens without disappearing. Product text: **“The choice stays yours.”** The copy refers to visible selection and explicit Enter, not automatic navigation. |
| 8.1–13.1 s | 486–785 | Git suggestions | Prompt types `git `, reveals the command catalog, then adds `c` to filter it. The panel shows `checkout`, `cherry-pick`, `clone`, and `commit` with their exact built-in descriptions. Selection moves to `commit`; Tab inserts `git commit`. No Enter is pressed. |
| 12.6–14.7 s | 756–881 | Project text | Product text: **“Your project speaks first.”** A narrow green focus line carries the eye into the next terminal state. |
| 14.2–18.3 s | 852–1097 | Project commands | In `~/dev/punk`, prompt types `pnpm run `. Six real fixture scripts appear and are labelled `PROJ`: `build`, `dev`, `format`, `lint`, `preview`, `test`. Selection moves to `dev`; Tab inserts `pnpm run dev`. Nothing executes. |
| 17.9–20.5 s | 1074–1229 | Outro | Terminal dissolves into the official Dirgo wordmark. Copy: **“Go anywhere. Stay in control.”** Final line: `github.com/RudySource/Dirgo`. |

## Website loop — `DirgoLoop`

Resolution is 1920×1080 at 60 FPS. Duration is 8 seconds (480 frames). The loop
reuses the terminal and suggestion components. It begins and ends on the same
near-black canvas for a clean repeat. A short **“Type less. Stay in flow.”**
statement resolves into the Git catalog, selection moves to `commit`, and Tab
inserts `git commit`. There is no CTA or final card.

## Terminal data

### Directory picker

```text
~/dev ❯ dgo sl

  Dirgo · directories
  › slash       ~/dev/slash
    slash-api   ~/dev/slash/services/api
    slash-web   ~/dev/slash/apps/web
    slash-docs  ~/dev/slash/docs

  ↑↓ select    Enter open    Esc close
```

Expected result after Enter:

```text
~/dev/slash ❯
```

### Git suggestions

```text
~/dev/slash ❯ git c

  Dirgo suggestions · 1–4 / 4
  › checkout    SUB    Switch branches or restore files
    cherry-pick SUB    Apply existing commits
    clone       SUB    Clone a repository
    commit      SUB    Record changes to the repository
```

All descriptions come from Dirgo's built-in Git command specification. The
film selects `commit` and inserts `git commit` with Tab. It does not
execute it.

### Project commands

```text
~/dev/punk ❯ pnpm run

  Dirgo suggestions · 1–6 / 6
    build       PROJ   package.json script · punk-web
  › dev         PROJ   package.json script · punk-web
    format      PROJ   package.json script · punk-web
    lint        PROJ   package.json script · punk-web
    preview     PROJ   package.json script · punk-web
    test        PROJ   package.json script · punk-web
```

Tab inserts `pnpm run dev`. Script bodies remain absent and no package manager
is invoked.

## Motion language

- Remotion frames are the only timeline source; no timers or runtime randomness.
- Typography enters with 12–18 px vertical travel, 10–16 px blur, and restrained
  0.985–1 scale changes.
- Terminal transitions use opacity, blur, and 0.965–1 scale. Camera movement is
  capped at 1.035 and follows only the active command or selected row.
- Typing uses a deterministic cadence table with slightly varied frame gaps.
- Selection movement uses high damping and no visible bounce.
- The cursor pauses before confirmation and insertion.
- Every scene contains a deliberate hold after its primary action.
- Blur is used for transitions only, never as permanent decoration.

## Visual system

- **Canvas:** `#030506`
- **Terminal surface:** `#0B0F12`
- **Raised row:** `#11181D`
- **Border:** `#263038`
- **Primary text:** `#F4F7F8`
- **Secondary text:** `#8E9AA5`
- **Dirgo accent:** `#20BF55`, used only for the cursor, focus line, source label,
  and small brand details.
- **Product type:** system sans stack with tight display tracking.
- **Terminal type:** system monospace stack with tabular metrics.
- **Spacing:** 8 px base rhythm; primary safe area is 160 px horizontally and
  110 px vertically so future aspect-ratio crops remain possible.
- **Terminal radius:** 30 px; one quiet border and one broad low-opacity shadow.

## Hierarchy and continuity

At every moment there is one primary focus: product text, typed command, active
result, or final wordmark. The terminal remains spatially consistent across
feature scenes. Text and terminal overlap during transitions, but the outgoing
layer loses contrast before the incoming layer moves. Background glow stays
localized behind the terminal and does not animate independently.

## Final frame

The exact repository wordmark appears on a clean neutral surface with:

```text
Go anywhere. Stay in control.
github.com/RudySource/Dirgo
```

No unverified installation command or performance claim appears in the film.

## Multidisciplinary review

- **Motion design:** one motion system, deliberate holds, no rotation, parallax,
  particles, bounce, or decorative camera movement.
- **Product design:** the film explains a complete workflow rather than a list
  of features; selection and insertion visibly preserve user agency.
- **Developer-tool marketing:** every phrase describes an observable behavior;
  there is no generic speed, AI, or productivity claim.
- **UI/UX:** command text remains readable at social-media scale; sources and
  keyboard actions use text labels rather than color alone.
- **Terminal realism:** prompts, commands, candidate sources, descriptions, and
  insertion behavior match Dirgo's current Zsh implementation and fixtures.

The review removed benchmark statistics, extra feature cards, decorative grid
lines, glass effects, and installation copy. The remaining story is shorter,
more specific, and easier to understand without sound.
