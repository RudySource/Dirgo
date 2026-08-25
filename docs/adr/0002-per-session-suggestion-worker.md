# Use workers scoped to one shell session

Latency-sensitive adapters may keep one lazily started Dirgo worker connected
through standard input and output. The worker belongs to that shell session,
exits when the channel closes, and falls back to bounded one-shot requests where
persistent communication is not reliable; Dirgo does not install a global
daemon or shared socket.
