# Security policy

## Supported versions

Until the first public release, only the latest commit is supported.

## Reporting a vulnerability

Do not open a public issue for a vulnerability that could expose local paths, execute commands, corrupt persistent state, or escape configured filesystem scope. Use the repository's private security-advisory channel once its GitHub owner is configured.

If no private channel is available, keep a minimal local reproduction and wait for maintainer contact details to be published; no email address is invented here.

## Security boundaries

- Dirgo performs normal navigation without network access or telemetry.
- The index is untrusted local data and must never become executable shell text.
- Actions must use direct process arguments, never `sh -c` path interpolation.
- Corrupt disposable index data may be quarantined and rebuilt. Persistent user state must be backed up before recovery.
