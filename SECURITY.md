# Security policy

Do not report vulnerabilities in a public issue. Contact the maintainer through
a private GitLab channel and include the affected version and minimal reproducer.

## Known release blocker

A historical commit contains `.monitor_sudo`. Deleting it from the current tree
did not remove it from Git history. Before publication:

1. Rotate the Linux password and the former GitLab token.
2. Remove `.monitor_sudo` and `.aether_sudo` locally without reading them.
3. Rewrite every Git ref with `git filter-repo` and force-push only after explicit
   coordination.
4. Re-protect `main`, clone the repository afresh, and run `gitleaks git`.

Current local development deliberately does not modify the GitLab repository.

## Runtime guarantees

- no stored sudo password or `sudo -S`;
- no shell interpolation for privileged operations;
- bounded subprocess output, timeouts and cancellation;
- terminal control and bidi characters sanitized before rendering;
- process actions validate PID and start time;
- container endpoint credentials are redacted from diagnostics.
