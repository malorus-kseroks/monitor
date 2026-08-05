# Release procedure

The repository is intentionally `publish = false` and no release job mutates a
remote repository. Before any public tag or artifact is published:

1. rotate the historical Linux password and GitLab token;
2. rewrite every remote ref that contains the secret, restore branch
   protection, and verify a fresh clone with `gitleaks git`;
3. run the Windows and Linux runtime matrix, PTY cleanup tests, hardware smoke
   tests, `cargo audit`, `cargo deny check`, ShellCheck and coverage >= 80%;
4. generate artifacts and the CycloneDX SBOM with `scripts/prepare-release.sh`
   or `scripts/prepare-release.ps1`;
5. sign `SHA256SUMS` with an externally stored Minisign key and verify every
   archive from a clean machine.

The scripts never create a private key. An unsigned local artifact is not a
release candidate.
