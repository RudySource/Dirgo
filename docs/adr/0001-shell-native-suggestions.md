# Use shell-native suggestion adapters

Dirgo integrates with each shell's editing and completion APIs instead of
starting the shell inside a PTY wrapper. This preserves normal shell ownership,
limits failures to optional suggestions, and accepts platform-specific
presentation differences in exchange for compatibility and safe recovery.
