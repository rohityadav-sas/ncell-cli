<div align="center">

![ncell-cli](assets/banner.svg)

<img alt="Written in Rust" src="https://img.shields.io/badge/Written_in-Rust-702DA0?style=for-the-badge&logo=rust&logoColor=white">
<img alt="ncell-cli v0.1.0" src="https://img.shields.io/badge/ncell--cli-v0.1.0-702DA0?style=for-the-badge&logo=rust&logoColor=white">
<img alt="MIT licensed" src="https://img.shields.io/badge/license-MIT-1F9D55?style=for-the-badge">

**[Read the documentation →](https://rohityadav-sas.github.io/ncell-cli/)**

</div>

## Example

```sh
ncell search "SMS" --category 50 --sort price
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

**From source**

Install [rustup](https://rustup.rs/) and Git, then clone the repository and install the executable on your `PATH`:

```sh
git clone https://github.com/rohityadav-sas/ncell-cli.git
cd ncell-cli
cargo install --path . --locked
ncell --help
```

**Prebuilt Windows executable**

Download the [Windows x86_64 release](https://github.com/rohityadav-sas/ncell-cli/releases/latest), extract the ZIP, and run `.\ncell.exe --help` from the extracted directory. Add that directory to your `PATH` to run it anywhere.