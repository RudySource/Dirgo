# Cache project commands off the input path

Dirgo parses bounded project manifests in a throttled background refresh and
publishes an immutable, content-fingerprinted snapshot per project root.
Completion requests read only the last published snapshot; a cold or stale
cache may temporarily omit project commands instead of delaying terminal input.

This keeps Zsh, Bash, Fish, and PowerShell responsive without a mandatory
daemon. It trades immediate first-keystroke discovery for predictable latency,
bounded local storage, deterministic invalidation, and failure isolation.
