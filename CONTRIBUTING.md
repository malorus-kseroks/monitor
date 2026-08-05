# Contributing

Use Rust 1.95+ and keep changes warning-free.

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo audit
cargo deny check
```

Providers must return typed `Unavailable`, `PermissionDenied` or `Error` states;
an empty vector must never conceal a provider failure. Rendering must remain pure
and must not access files, sockets, D-Bus or external commands.
