# Changelog

## v0.3.1 � 2026-10-13

- Restore the server sort field when browsing and searching plans so the full available catalog is returned.

## v0.2.0 — 2026-10-12

- Add, list, switch and remove saved accounts without logging in again.
- Protect credential updates with a shared lock and verify saved writes.
- Keep Windows credentials under `ncell-cli` and migrate temporary v2 storage once.
- Show token metadata and clearer balances with pack details and Nepal time.
- Reuse game sessions during repeated play and identify your leaderboard entry.
- Treat already checked-in results as successful and format JSON consistently.
- Reorganize documentation, separate command examples and preserve sidebar scrolling.
- Provide Windows, Linux and macOS downloads for x86_64 and ARM64.

## v0.1.0 — 2026-10-12

### Features

- Log in with an SMS code.
- Store login details securely.
- Check balances, profile and SIM status.
- Search plans and sort by price or validity.
- Buy packs.
- Send free SMS.
- Refresh tokens automatically or manually.
- Get JSON output, import sessions and check connectivity.
- Read the online documentation.

### Downloads

- Windows x86_64: download and extract the ZIP archive, then run `ncell.exe`.
- macOS and Linux users currently build from source; Linux requires an unlocked Secret Service provider.
