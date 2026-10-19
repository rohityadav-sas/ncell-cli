<div align="center">

![ncell-cli](assets/banner.svg)

<img alt="Written in Rust" src="https://img.shields.io/badge/Written_in-Rust-702DA0?style=for-the-badge&logo=rust&logoColor=white">
<img alt="Latest ncell-cli release" src="https://img.shields.io/github/v/release/rohityadav-sas/ncell-cli?style=for-the-badge&color=702DA0&logo=rust&logoColor=white">
<img alt="MIT licensed" src="https://img.shields.io/badge/license-MIT-1F9D55?style=for-the-badge">

**[Read the documentation →](https://rohityadav-sas.github.io/ncell-cli/)**

</div>

## Example

```sh
ncell plans search "SMS" --category 50 --sort price
```

### Output

```text
ID      Plan                             Price    Validity
--------------------------------------------------------------
984     1 Day: 100 All Net SMS          Rs. 15       1 DAY
985     7 Day: 300 All Net SMS          Rs. 45       7 DAY
986     28 Day: 300 All Net SMS         Rs. 98      28 DAY
987     28 Day : 850 All Net SMS       Rs. 125      28 DAY
```

## Interactive mode

```sh
ncell
```

Live dropdown completion, argument hints, script files and plan categories are available without shell setup. Commands execute against your saved account. See the [interactive guide](https://rohityadav-sas.github.io/ncell-cli/#interactive).

## Installation

**Quick install (Linux / macOS)**

```sh
curl -fsSL https://raw.githubusercontent.com/rohityadav-sas/ncell-cli/main/install.sh | sh
```

**Quick install (Windows PowerShell)**

```powershell
irm https://raw.githubusercontent.com/rohityadav-sas/ncell-cli/main/install.ps1 | iex
```

The installer downloads the right prebuilt binary for your OS and CPU from the [latest release](https://github.com/rohityadav-sas/ncell-cli/releases/latest) and needs no admin rights or sudo.

| OS | Installs to | PATH |
| --- | --- | --- |
| Linux / macOS | `~/.local/bin/ncell` | Prints the line to add if the folder is missing from your `PATH` |
| Windows | `%LOCALAPPDATA%\Programs\ncell-cli\ncell.exe` | Added to your user `PATH` automatically |

Linux and macOS need `curl` or `wget`, plus `unzip`.

Install a specific version or folder with environment variables:

```sh
curl -fsSL https://raw.githubusercontent.com/rohityadav-sas/ncell-cli/main/install.sh | NCELL_VERSION=v0.5.4 sh
```

```powershell
$env:NCELL_VERSION = 'v0.5.4'; irm https://raw.githubusercontent.com/rohityadav-sas/ncell-cli/main/install.ps1 | iex
```

`NCELL_INSTALL_DIR` changes the install folder in both scripts. On Linux and macOS it must be an absolute path; on Windows, relative paths are converted to full paths before installation. Re-run the same command at any time to update.

To uninstall, delete the `ncell` binary from the install folder. On Windows, also remove that folder from your user `PATH`.

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

**From source**

Install [rustup](https://rustup.rs/) and Git, then clone the repository and install the executable on your `PATH`:

```sh
git clone https://github.com/rohityadav-sas/ncell-cli.git
cd ncell-cli
cargo install --path . --locked
ncell --help
```

## Prebuilt binaries

Download the ZIP archive for your platform from [Releases](<https://github.com/rohityadav-sas/ncell-cli/releases/latest>).

| Platform | Architecture | Archive suffix |
| --- | --- | --- |
| Windows | x64 | `windows-x86_64.zip` |
| Windows | ARM64 | `windows-arm64.zip` |
| Linux | x64 | `linux-x86_64.zip` |
| Linux | ARM64 | `linux-arm64.zip` |
| macOS | Intel | `macos-x86_64.zip` |
| macOS | Apple Silicon | `macos-arm64.zip` |

### Running the binary

1. Extract the downloaded ZIP archive.
2. Run the executable using the appropriate command:

**Windows (PowerShell):**

```
.\ncell.exe --help
```

**Linux and macOS:**

```
chmod +x ncell
./ncell --help
```
Linux credential storage requires an unlocked Secret Service provider, such as GNOME Keyring or KWallet with Secret Service enabled.

3. (Optional) Add the directory containing the executable to your `PATH` to run `ncell` from anywhere.
