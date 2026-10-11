<div align="center">

![ncell-cli](assets/banner.svg)

OTP login · balances · plans · subscriptions · free SMS

[![CI](https://github.com/rohityadav-sas/ncell-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/rohityadav-sas/ncell-cli/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/Written_in-Rust-702DA0?style=flat-square&logo=rust&logoColor=white)

**[Read the documentation →](https://rohityadav-sas.github.io/ncell-cli/)**

</div>

## Get started

Download the [Windows release](https://github.com/rohityadav-sas/ncell-cli/releases/latest), extract the ZIP, and run `ncell.exe` from a terminal. See the [changelog](CHANGELOG.md).

To build from source, install [stable Rust](https://rustup.rs/), then:

```sh
git clone https://github.com/rohityadav-sas/ncell-cli.git
cd ncell-cli
cargo install --path . --locked
ncell login 98XXXXXXXX
ncell balance
```

No Android device is needed. Tokens are saved in your native credential store; Linux requires an unlocked Secret Service provider.

## Package managers

**Scoop (Windows)**

```powershell
scoop bucket add ncell-cli https://github.com/rohityadav-sas/ncell-cli
scoop install ncell-cli/ncell-cli
```

**Homebrew (macOS / Linux, builds from source)**

```sh
brew tap rohityadav-sas/ncell-cli https://github.com/rohityadav-sas/ncell-cli
brew install rohityadav-sas/ncell-cli/ncell-cli
```

WinGet [submission #450764](https://github.com/microsoft/winget-pkgs/pull/450764) is pending for `RohitYadav.NcellCLI`; it is not yet available in the community source. crates.io publishing is pending authentication. See the [packaging guide](packaging/README.md).

## A few commands

```sh
ncell plans --list-categories
ncell search "SMS" --category 50 --sort validity:desc,price:asc
ncell profile --json
ncell send-free-sms 98XXXXXXXX "Hello"
```

Use `--json` for structured output. Authenticated commands refresh and retry once after a confirmed access-token expiry.

Subscriptions activate immediately and paid packs charge your balance. See the [command reference](https://rohityadav-sas.github.io/ncell-cli/#commands) for usage and the [session guide](https://rohityadav-sas.github.io/ncell-cli/#sessions) for refresh behavior.

## Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Documentation lives in `docs/`, built with plain HTML, CSS and JavaScript. GitHub Actions deploys it to Pages. Local integration tests and the optional Android token bridge are excluded from this repository.

An independent project for Ncell Nepal. Not affiliated with or endorsed by Ncell.





