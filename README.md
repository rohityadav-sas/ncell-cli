<div align="center">

![Ncell CLI](assets/banner.svg)

<img src="assets/icon.svg" width="64" height="64" alt="Ncell CLI icon" />

### A little terminal. Your whole Ncell account.

OTP login · balances · plans · subscriptions · free SMS

[![CI](https://github.com/rohityadav-sas/ncell-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/rohityadav-sas/ncell-cli/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/Rust-2024-F472B6?style=flat-square&logo=rust&logoColor=white)
![Platforms](https://img.shields.io/badge/Windows_·_macOS_·_Linux-7C3AED?style=flat-square&logo=gnometerminal&logoColor=white)
![Storage](https://img.shields.io/badge/Credentials-Native_keyring-16A34A?style=flat-square&logo=keepassxc&logoColor=white)

[Get started](#-get-started) · [Commands](#-commands) · [Search & sort](#-find-your-next-pack) · [Development](#-small-by-design)

</div>

> An independent Rust CLI for Ncell Nepal. Not affiliated with or endorsed by Ncell. The icon is original artwork for this project.

## ✨ Made for the terminal

| | What you get |
| :-- | :-- |
| 🔐 **Sign in once** | SMS OTP login with a stable device ID and hidden OTP input. |
| 💳 **Know your account** | Credit, data, voice and SMS balances, plus subscriber profile and SIM status. |
| 🔎 **Find your pack** | Server-side search within a tag, readable tables and ordered sorting. |
| 📦 **Subscribe directly** | Choose a plan ID from its catalog tag. Paid packs charge your account. |
| 💬 **Send free SMS** | Use the account's available free SMS allowance. |
| 🔄 **Stay signed in** | Refresh tokens automatically after the confirmed access-token expiry response. |
| 🧩 **Script it** | `--json` for structured output on every command. |

## 🚀 Get started

Install [stable Rust](https://rustup.rs/), then build and install:

```sh
git clone https://github.com/rohityadav-sas/ncell-cli.git
cd ncell-cli
cargo install --path . --locked
ncell login 98XXXXXXXX
ncell balance
```

Enter the SMS code at the prompt. No Android device or root access is needed.

Interrupted the login? Resume with the same code without requesting another SMS:

```sh
ncell login 98XXXXXXXX --verify
```

Numbers may include `+977`. For a fresh code, run normal `login` again.

<details>
<summary><b>Prefer a local executable?</b></summary>

```sh
cargo build --release --locked
```

Windows: `./target/release/ncell.exe` · macOS/Linux: `./target/release/ncell`

</details>

## 🧭 Commands

| Command | Purpose |
| :-- | :-- |
| `ncell login <number>` | Request an OTP and sign in. |
| `ncell login <number> --verify` | Complete a pending login. |
| `ncell status` | Show the local saved session; does not verify token validity. |
| `ncell balance` | Show credit, data, voice and SMS totals. |
| `ncell profile` | Show account details and SIM status. |
| `ncell plans` | List available plans; default tag is 66. |
| `ncell plans --list-tags` | List current tag IDs and categories. |
| `ncell search <keyword> --tag <tag>` | Search the selected catalog on the server. |
| `ncell subscribe --tag <tag> --id <id>` | Purchase or activate a listed pack. |
| `ncell send-free-sms <number> "Hello"` | Send a free SMS, subject to account allowance. |
| `ncell refresh` | Explicitly rotate saved tokens. |
| `ncell doctor` | Check HTTPS connectivity without sending an OTP. |
| `ncell logout` | Delete the local session and device ID. |
| `ncell import-session` | Import session JSON from standard input. |

Use `ncell <command> --help` for the available options.

## 🔎 Find your next pack

```sh
ncell plans --list-tags
ncell plans --tag 50
ncell search "SMS" --tag 50 --sort validity:desc,price:asc
ncell search "Super" --sort price
```

| Common tag | Catalog |
| :-- | :-- |
| `50` | SMS |
| `61` | Voice |
| `66` | Data & Voice; default |

Search is scoped to the selected tag. It does not search every catalog at once. Other tags are resolved using the server's category list; unsupported categories return an error.

```text
ID      Plan                             Price  Validity
--------------------------------------------------------------
986     28 Day: 300 All Net SMS         Rs. 98  28 DAY
987     28 Day : 850 All Net SMS       Rs. 125  28 DAY
985     7 Day: 300 All Net SMS          Rs. 45  7 DAY
984     1 Day: 100 All Net SMS          Rs. 15  1 DAY
```

*Example output; availability and prices depend on the account and server.*

Sort by `price` or `validity`. Direction defaults to `asc`; add `:desc` to reverse it. Keys apply from left to right, and duration comparisons account for hours and days.

```sh
# Longest validity first, then lowest price within that duration
ncell plans --sort validity:desc,price:asc

# Repeated options work too
ncell plans --sort validity:desc --sort price:asc

# Alternative server-search syntax
ncell plans --search "Combo" --sort price
```

To subscribe, use the plan ID and the same tag that listed it:

```sh
ncell subscribe --tag 50 --id 984
```

**This submits a subscription request. Paid packs charge your Ncell balance.**

## 🧩 JSON when you need it

```sh
ncell balance --json
ncell profile --json
ncell search "SMS" --tag 50 --sort price --json
ncell plans --list-tags --json
```

`--json` works before or after the subcommand. Catalog and subscription commands return their server response; balance/profile return their response object. Sorting also applies to JSON catalogs. Local operations return structured status without tokens.

Server rejections return the full response JSON on stdout and a nonzero exit status. Local/network errors return JSON on stderr. Argument errors and help use Clap's standard formatting. OTP entry remains interactive.

## 🔐 Sessions that belong in your keyring

| Platform | Credential backend |
| :-- | :-- |
| Windows | Credential Manager |
| macOS | Keychain |
| Linux | Secret Service, such as GNOME Keyring or KWallet |

The session is stored under service `ncell-cli`, entry `session`. Linux requires a session D-Bus and an unlocked Secret Service provider; there is no plaintext fallback. One account and one pending login are stored. OTPs are not saved, and failed logins preserve the existing authenticated account.

Authenticated commands refresh and retry **once**, only after HTTP 401 / `LGN2001`. Rotated tokens are saved before retrying. Other server errors and network failures are not automatically replayed. `LGN2003` remains unconfirmed and does not trigger refresh. Logging out removes local credentials without revoking server tokens.

Profile summaries map `B01` to Active, `B03` to All Services Blocked and `B04` to Outgoing Blocked. Unknown codes are preserved. Profile JSON may contain sensitive account information, including PIN/PUK details; keep it private.

<details>
<summary><b>Optional Android token bridge · Windows</b></summary>

```powershell
.\sync_token.ps1 mobile-to-cli
.\sync_token.ps1 cli-to-mobile
```

This optional script requires a rooted Android device, adb, and the separately supplied sibling `../bypass/scripts/pull_tokens.ps1` and `push_tokens.ps1` helpers with their Python setup. The bridge script and those helpers are **not included** in this repository. Build the release CLI first.

Importing from mobile preserves the number and device ID. Pushing tokens restarts the mobile app. Neither direction requests an OTP or refreshes tokens. If the app shares the CLI session, synchronize after token rotation; synchronization is not automatic.

</details>

<details>
<summary><b>Import an existing session</b></summary>

`import-session` reads this structure from standard input:

```json
{
  "device_id": "<matching login device ID>",
  "account": {
    "msisdn": "<10-digit number>",
    "access_token": "<access token>",
    "refresh_token": "<refresh token>",
    "authenticated_at": "<ISO UTC timestamp>"
  }
}
```

PowerShell: `Get-Content -Raw session.json | ncell import-session`

Unix shell: `ncell import-session < session.json`

Keep session files private. Prefer native credential storage for normal use.

</details>

## 🦀 Small by design

```text
src/
├── api/          HTTP transport, request builders and response handling
├── session/      Session models and native credential storage
├── auth.rs       Login, device identity and bounded refresh
├── cli.rs        Arguments, tables and JSON output
├── error.rs      Safe errors and authentication-code labels
├── lib.rs        Reusable library
└── main.rs       Process entry point

assets/           Original SVG icon and static banner
```

Built with `clap`, `reqwest`, `serde`, `keyring`, `rpassword`, `chrono`, `uuid` and `thiserror`. No custom HTTP stack, JWT parser or plaintext credential database.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

For a local checkout that includes the private integration tests, an optional native credential-store check creates and removes an isolated test entry:

```sh
cargo test --locked --test credential_store -- --ignored
```

CI is configured for Windows, macOS and Linux. Unit tests are included in the source. The local integration-test directory and token bridge are excluded from this repository. Live account operations require a real subscriber.

<details>
<summary><b>Request minimization: what is verified?</b></summary>

Live omission checks established the retained common request-header fields for OTP, balance, profile, tags, catalogs and refresh. Profile uses an empty request object; catalog requests omit empty keywords and redundant sort direction. Tags/profile retain app version, and refresh retains the matching device ID. HTTP requests use JSON content type and bearer authentication where required, plus library-managed transport headers.

The SMS body contains only destination, content and `isConfirm: 0`; removing `isConfirm` failed in live testing. Individual common-header omissions for SMS and further subscription-field omissions remain unverified. These requests are not claimed to be globally minimal. Server requirements can change.

</details>

<div align="center">

**Less ceremony. More command line.**

<sub>Original artwork · built for the terminal</sub>

</div>


