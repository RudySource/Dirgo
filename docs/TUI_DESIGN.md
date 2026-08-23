# Dirgo picker design

The picker serves terminal-first developers who know part of a destination name. Its single job is to make the intended directory obvious and selectable without making navigation feel like opening a separate application.

## Direction

Dirgo uses the terminal's own background and typography. It borrows its visual language from paths themselves: indentation, separators, a current position, and a clear destination. It does not imitate a desktop window, depend on patched fonts, or fill the viewport with branded color.

## Tokens

- `primary`: terminal default foreground — paths and essential labels.
- `muted`: terminal dim modifier — parents, hints, and metadata.
- `accent`: ANSI cyan by default — query cursor, match emphasis, and the navigation rail.
- `selection`: bold text plus the accent rail; reversed color is reserved for low-color terminals.
- `warning`: ANSI yellow — stale or inaccessible destinations.
- `error`: ANSI red — actionable failure only.

Typography is terminal-native: bold basename is the display role, normal path is the body role, and dim metadata is the utility role. No custom font is required.

## Signature

The selected destination is marked by a thin accent navigation rail (`│`) rather than a full-width colored bar. It reads as “you are here” and keeps long paths legible in every terminal theme. This is the one expressive element; borders, icons, and color remain restrained.

## Layouts

Wide terminals expose context without additional input:

```text
 Dirgo   › pun
 ──────────────────────────────────────────────────────────────────
 │ Punk                         │ ~/Developer/Projects/Punk
   ~/Developer/Projects         │ Rust project

   punk-api                     │ visited 28 times
   ~/Developer/Services         │ bookmark: work
 ──────────────────────────────────────────────────────────────────
 ↑↓ move   Enter go   Tab preview   Esc close
```

Medium terminals keep metadata under each basename:

```text
 Dirgo   › pun
 │ Punk
   ~/Developer/Projects
   punk-api
   ~/Developer/Services
 ↑↓ move   Enter go   Esc close
```

Small terminals remove the title and secondary hints before truncating the destination:

```text
 › pun
 │ Punk
   ~/Developer/Projects
 ↑↓  Enter  Esc
```

## Interaction rules

- Input and selection respond immediately; decoration never delays matching.
- Preview is absent below the width threshold and never leaves a blank column.
- Footer promises only actions available in the current build and platform.
- `NO_COLOR` removes semantic color without removing bold/dim hierarchy or the selection rail.
- Empty states state the condition and the next useful action.
- Raw mode and cursor visibility are guarded so every success, cancellation, I/O error, and panic path can restore the terminal.

The deliberate risk is avoiding the familiar reversed full-row selection. The accent rail is quieter and more specific to navigation; render and real-terminal tests must confirm that it remains visible across common themes.
