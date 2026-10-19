# Contributing

Thanks for your interest in contributing to termbonsai.

## Development

1. Fork / clone the repository
2. Install a stable Rust toolchain (https://rustup.rs)
3. Create a branch: `git switch -c feat/your-change`
4. Make your change and verify locally:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test --all
   cargo build --release
   ```
5. Commit and open a pull request

## Notes

- `TERMBONSAI_NOW` (Unix seconds) sets the current time and
  `TERMBONSAI_STATE` the state file, which is how the tests live a tree's whole
  life in a moment. Try a change with them before waiting days for it.
- The tree's shape comes only from its seed. Changing `src/tree.rs` changes
  every existing tree's shape, so say so in the pull request.
- Draw your own art; do not paste art or code from other tree programs.
- Keep termbonsai free of dependencies.
