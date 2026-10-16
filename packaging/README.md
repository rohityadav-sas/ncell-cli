# Package publishing

The Windows ZIP contains `ncell.exe` and the MIT license. Its checksum is pinned in the Scoop and WinGet manifests. Do not replace a published archive after a package manager accepts its checksum; use a new version for future changes.

## WinGet

`winget/0.1.0/` contains the validated manifest set for `RohitYadav.NcellCLI`. Submission: https://github.com/microsoft/winget-pkgs/pull/450764. Community-source availability requires Microsoft to merge the submission. The owner must review and accept Microsoft's CLA in the bot comment before the submission can proceed. After acceptance:

```powershell
winget install --id RohitYadav.NcellCLI --exact
```

Validate locally with `winget validate --manifest packaging/winget/0.1.0`. Installation from a local manifest requires the administrator-controlled `LocalManifestFiles` setting. This setting was not changed during publishing; the local install test was blocked by it. The Windows CI download, checksum, MIT-license, version and help checks passed in run 38167578248. Local download checks timed out; the published archive was verified by CI.

## Scoop

The repository itself is a bucket. The manifest is `bucket/ncell-cli.json`.

```powershell
scoop bucket add ncell-cli https://github.com/rohityadav-sas/ncell-cli
scoop install ncell-cli
```

Scoop was not installed on the publishing machine. The Windows archive check is configured in CI; a full Scoop installation was not run here. No central-bucket approval is needed to use this custom bucket.

## Homebrew

The repository itself is a custom tap. Specify its remote URL because its name does not use the usual `homebrew-` prefix.

```sh
brew tap rohityadav-sas/ncell-cli https://github.com/rohityadav-sas/ncell-cli
brew install ncell-cli
```

The formula builds the MIT-licensed release source archive with Cargo. The archive checksum is pinned. This is a custom tap, not a homebrew/core listing. The macOS Homebrew installation and formula tests passed in run 38167412633. Linux additionally needs a working Secret Service at runtime.

## crates.io

Cargo metadata, the MIT license and an explicit source-file inclusion list are configured. `cargo package --locked` verifies the package builds and excludes local token scripts, integration tests, captures and documentation assets.

Publishing is automated behind a one-click gate: creating a GitHub release
runs the `publish-crates` job, which waits for the `crates-io` environment
approval, skips versions already on the registry, dry-runs, then runs
`cargo publish --locked`. Publishes are permanent, so no version ships
unattended. Until then, use `cargo install --git https://github.com/rohityadav-sas/ncell-cli --locked` or build from a clone.

One-time setup (owner only):

1. Create a token at <https://crates.io/settings/tokens>, restricted to the
   `ncell-cli` crate with an expiry date.
2. Add it as an Actions secret (repository Settings, Secrets and variables,
   Actions, environment `crates-io`, secret `CARGO_REGISTRY_TOKEN`), or run:
   `gh secret set CARGO_REGISTRY_TOKEN --repo rohityadav-sas/ncell-cli --env crates-io`

## Updating

Bump the `Cargo.toml` version and push to `main`. The auto-release workflow
tags the new version and creates the GitHub release with draft notes; edit the
notes on the release page afterwards. Creating the release starts the binary
builds, and the `rehash` job then points Homebrew, Scoop and WinGet at it and
dispatches Homebrew validation. `cargo publish` stays manual (registry login),
as does `npm publish` on the TypeScript side (login plus authenticator code).

Release assets should remain immutable after package-manager publication.


