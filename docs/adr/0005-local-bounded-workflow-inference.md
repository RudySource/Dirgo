# Keep workflow inference local, bounded, and insertion-only

Dirgo derives exact command transitions inside the existing private command-history database only after separate workflow opt-in. This trades generalized or automatic workflows for deterministic local evidence, project/session isolation, bounded latency and storage, and an auditable guarantee that a next action can only be inserted into the shell buffer—never submitted, queued, retried, or executed by Dirgo.
