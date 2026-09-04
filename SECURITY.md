# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| Latest `0.8.x` release | Yes |
| `0.7.x` and older | No |

Security fixes are released from the current `main` branch. Users should update
to the latest published release before reporting a problem.

## Reporting a vulnerability

Do not open a public issue for a vulnerability that could expose local paths, execute commands, corrupt persistent state, or escape configured filesystem scope. Use GitHub's **Security → Report a vulnerability** flow for this repository.

If the private reporting flow is unavailable, keep a minimal local reproduction and do not publish sensitive details in an issue, discussion, log, or screenshot.

## Security boundaries

- Dirgo performs normal navigation without network access or telemetry.
- The index is untrusted local data and must never become executable shell text.
- Actions must use direct process arguments, never `sh -c` path interpolation.
- Corrupt disposable index data may be quarantined and rebuilt. Persistent user state must be backed up before recovery.
- Command history and Workflow Intelligence are separate opt-ins. Learned and
  saved workflows are insertion data only and never authorize automatic execution.
- Workflow exports redact project paths by default, refuse symlink targets, use
  private permissions, and require `--force` before replacement.
