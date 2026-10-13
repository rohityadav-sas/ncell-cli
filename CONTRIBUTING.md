# Development

## Sources of truth

- `Cargo.toml` owns the Rust package version. Clap reads it automatically. Cargo generates the matching lockfile entry.
- `python scripts/build-docs.py` generates ignored `docs/version.json` from that manifest. GitHub Pages runs it automatically; the website reads it for all version labels. Serve `docs/` over HTTP when previewing locally.
- The README badge reads the latest GitHub release. Release builds reject a tag or executable version that differs from the manifest. Historical changelog entries and package-manager manifests describe their own releases.
- `src/api/mod.rs` owns the default plan category and game-service base URL. CLI and API code reuse them.
- `docs/` owns the shared CLI documentation, navigation, theme and response-code reference. The TypeScript repository imports these through its documentation sync script.

## Version updates

Change the package version in `Cargo.toml`, run `cargo check` to update `Cargo.lock`, and commit both. Create the matching `vX.Y.Z` GitHub release after checks pass. No README or website version edits are needed.

## Checks

```sh
python scripts/build-docs.py
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```
