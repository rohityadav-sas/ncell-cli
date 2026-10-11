<div align="center">

![ncell-cli](assets/banner.svg)

OTP login · balances · plans · subscriptions · free SMS

[![CI](https://github.com/rohityadav-sas/ncell-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/rohityadav-sas/ncell-cli/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/Written_in-Rust-702DA0?style=flat-square&logo=rust&logoColor=white)

**[Read the documentation →](https://rohityadav-sas.github.io/ncell-cli/)**

</div>

## Installation

**WinGet (Windows)**

```powershell
winget install --id RohitYadav.NcellCLI
```

**Scoop (Windows)**

```powershell
scoop bucket add ncell-cli https://github.com/rohityadav-sas/ncell-cli
scoop install ncell-cli
```

**Homebrew (macOS / Linux)**

```sh
brew tap rohityadav-sas/ncell-cli https://github.com/rohityadav-sas/ncell-cli
brew install ncell-cli
```

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





