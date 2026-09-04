# Workflow Intelligence

Dirgo's Workflow Engine answers one bounded local question: after a completed command in this project, which previously observed command usually came next? It learns exact sequences from separately opted-in command history and exposes one next action for inspection or insertion. It never invents, submits, queues, retries, or executes a command.

## Product boundary

Workflow suggestions and command history are independent settings. Command history must already be enabled before workflow inference can be enabled; Dirgo never changes that prerequisite implicitly. Disabling workflows stops ranking immediately without deleting command events, learned transitions, or saved workflows.

A workflow contains two to eight filtered command texts and belongs to one canonical project root or the explicit global scope. Project identity is the canonical path, never a basename. Learned workflows are disposable derived data. Saved workflows are explicit user-owned definitions, but being saved is not a claim that their steps are safe to execute automatically.

Every output preserves the shell-ownership contract:

- a next action is insertion data only;
- an empty prompt never opens an automatic panel;
- `Tab`, `Ctrl+F`, or Palette selection may insert one visible command;
- Enter remains owned by the shell and submits only text already visible in its buffer;
- there is no workflow runner, active queue, daemon, or hidden continuation.

## Storage model

Workflow data extends the existing private `suggestions.redb` database from schema 2 to schema 3. It does not create a second command-history store.

Schema 3 retains the existing metadata, completed events, and command aggregates and adds:

- `workflow_transitions_v1`: bounded, rebuildable aggregates for one- and two-command predecessor contexts;
- `saved_workflows_v1`: explicitly named, user-owned sequences;
- metadata recording migration and the last deterministic transition rebuild.

Migration from schema 2 creates and validates the new tables in one redb transaction without rewriting existing event or aggregate rows. A failed transaction leaves schema 2 readable. Reopening schema 3 is idempotent, and an unknown future schema is preserved and rejected. Downgrading to 0.7 requires exporting or clearing the schema-3 history because 0.7 cannot interpret it.

The database remains private and symlink-safe. Rows, tables, command bytes, step counts, evidence sessions, retained age, and query results all have hard limits. Malformed optional workflow rows fail closed without weakening unrelated suggestion providers.

## Learning contract

Only consecutive eligible completed events may form a transition. Both events must have the same canonical scope and non-empty shell session, and their start times must be no more than 30 minutes apart. A rejected command, project boundary, session boundary, missing event caused by retention, duplicate event, or privacy gap breaks the sequence.

The engine records one-command and, when available, two-command predecessor contexts. The outcome attached to a predecessor is the outcome of its most recent command. A learned next action becomes eligible only after at least three observations from at least two distinct sessions. Evidence session identifiers are unique and capped at eight.

Transition updates commit with their completed event. A deterministic rebuild can replace all learned transitions from retained events without changing saved workflows. Pruning keeps at most 10,000 learned transitions for 180 days; saved workflows are capped at 256, with two to eight steps each and the existing command byte limit.

## Ranking

Ranking uses fixed integer components so ties are stable across platforms. It considers:

1. exact canonical project scope before global fallback;
2. saved workflow precedence over equivalent learned evidence;
3. a sufficiently supported two-command predecessor before a one-command predecessor;
4. observation and distinct-session support;
5. successful next-command outcomes and recency, with recent failure reducing but not erasing inspectable evidence;
6. stable command and identifier ordering for deterministic ties.

Current-project `PROJ` declarations keep precedence when their inserted text is identical. Global next actions are bounded so they cannot crowd out every project-local candidate. Confidence is a deterministic score from 0 through 1000, not a probability or safety guarantee. User-facing reasons report evidence without exposing filesystem paths.

## Read path and failure isolation

Suggestion workers consume a small immutable `WorkflowSnapshot`. The snapshot reloads only when the storage stamp changes; per-keystroke ranking performs no database write, migration, filesystem crawl, subprocess, network request, or full event scan.

If history is disabled, workflow inference is disabled, the database is locked, corrupt, migrating, or from a future schema, the Workflow provider contributes no candidates and all other providers continue normally. A failure is observable through management commands and `doctor`, not through latency or noisy errors while typing.

## Shell suggestions and Workspace Palette

Shell suggestions label eligible next actions as `NEXT` and show compact evidence such as `Next in this project · 6 times · 83% successful`. A prefix is required before they appear.

Workspace Palette places Workflows after Tasks and before Git. Learned and saved choices show source, scope, step count, and evidence text. Wide layouts preview the complete sequence vertically and highlight only the next step; compact layouts retain a one-line next-step explanation. Every preview states `Inserted, never executed`.

Palette preview computation belongs to one session-owned latest-request worker. New selection generations supersede obsolete requests, and closing Palette stops and joins the worker. This prevents the current per-selection detached-thread pattern from accumulating stale filesystem work.

## Management and export

`dgo workflows` exposes enablement, status, next-action inspection, listing, saving, renaming, removal, learned-data clearing, and versioned JSONL export. Read commands are deterministic and non-mutating.

`save --last N` uses the current `DGO_SESSION_ID` and scope, displays all eligible steps, and requires one confirmation; non-interactive use requires `--yes`. Command text is read from the private store and never placed in process arguments. Names are unique per scope, contain 1–64 visible characters, and reject controls and bidirectional overrides.

Exports are private, atomic, path-redacted by default, symlink-safe, and non-overwriting unless `--force` is explicit. Clearing learned transitions never clears completed-command history or saved workflows, and removing a saved workflow never edits or executes its steps.
