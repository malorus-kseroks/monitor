# Troubleshooting

Run `kernox-monitor doctor --json` before reporting a problem. It prints provider
states without exposing credentials from container URIs.

- Docker unavailable: start Docker Desktop/Engine or a Podman API service and
  verify the current user can access its socket. Never use `chmod 666`.
- Linux page unavailable: install/configure the relevant optional provider; the
  core TUI must continue to work without it.
- Permission denied: fix group, ACL or daemon policy. Background refresh never
  invokes sudo.
- Damaged terminal after a forced process kill: run `reset` on Unix or reopen the
  Windows Terminal tab. Normal errors and panics are handled by the RAII guard.
