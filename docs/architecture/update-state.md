# Update state architecture

Dirgo answers update questions from local state first. An interactive
`dgo --version` never waits for GitHub: it prints the installed version, reads
the last valid release response, tries to start one detached checker when due,
and renders what actually happened. Redirected version output returns before
path discovery and remains exactly one line.

## Independent facts

Update state keeps three questions separate:

1. **Version relation:** is the installed build behind, equal to, or ahead of
   the last known stable release?
2. **Freshness:** is that release response fresh, stale, or missing?
3. **Refresh disposition:** was checking unnecessary, started, already active,
   backing off, disabled, or unable to start?

A stale response is old knowledge, not absent knowledge. If it names a newer
stable release, Dirgo continues to show the available update with a cached
qualification. A cached current version is not described as confirmed current
until a fresh successful response exists.

Only exact stable `major.minor.patch` versions enter the cache. Invalid,
oversized, control-containing, symlinked, or non-file cache data is treated as
unknown and is never rendered as release text.

## Local files

The update subsystem uses three bounded private files below Dirgo's cache and
state directories:

- `update.json` stores `latest_version` and `checked_at`. The timestamp means
  the last successfully validated release response, never an attempt.
- `update-check` stores a short running lease, bounded failure backoff, or
  completed-at marker. Processes coordinate through an exclusive file lock.
- `update-notifications-disabled` contains the exact canonical disabled marker.

Cache publication uses a private same-directory temporary file followed by
platform-aware replacement. Attempt-state writes occur while holding the
exclusive lock. State readers reject symlinks, non-files, oversized content,
and malformed values rather than letting them silently change notification
settings.

## Scheduling and failure behavior

A successful response remains fresh for 24 hours. When it is stale or missing,
the foreground process claims a five-minute attempt lease and starts the hidden
checker with null standard streams. Other simultaneous processes observe the
lease and do not create another child.

The child publishes the release cache only after transport, JSON, and stable
version validation all succeed. It then marks the attempt complete. A fetch
failure records only the category `fetch-failed` and retries after 15 minutes;
a spawn failure records `spawn-failed` and retries after at most 60 seconds.
No response body, command output, path, or credential is stored in attempt
state.

Future timestamps and clock rollback are never accepted as unlimited freshness,
leases, or backoff. A running timestamp is valid for at most five minutes from
the observing clock; a backoff is honored only when its remaining duration is
within the configured 15-minute maximum. Malformed and expired attempt data is
replaced while the coordination lock is held.

`DGO_DISABLE_UPDATE_CHECK` and the persistent notification setting use the same
reader. Disabled checking renders no navigation notice and starts no network
request.

## Presentation contract

The version line remains primary. Status uses a compact textual block without
animation, cursor movement, or terminal ownership. “Checking” appears only
after a refresh started or an existing active lease was observed. Color and
Unicode are optional decoration; ASCII, no-color, narrow terminals, and
`TERM=dumb` retain the same words and actions.

Process and PTY tests use a local fake release transport. They verify stale
known updates, missing/current/ahead relations, clock edges, malformed and
hostile state, concurrent claims, spawn/fetch backoff, successful publication,
prompt and buffer restoration, ASCII fallback, and immutable one-line piped
output without depending on live network availability.
