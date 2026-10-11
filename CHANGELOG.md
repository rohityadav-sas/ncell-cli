# Changelog

## v0.1.0 — 2026-10-12

First public release of ncell-cli.

### Features

- SMS OTP login with hidden code entry, resumable verification and a stable device ID.
- Native credential storage on Windows, macOS and Linux.
- Credit, data, voice and SMS balances, subscriber profiles and readable SIM status.
- Catalog tags, server-side plan search and multi-field price/validity sorting.
- Plan subscriptions using current catalog metadata and the saved device identity.
- Free SMS sending using the account's allowance.
- Manual token refresh and one automatic refresh/retry after HTTP 401 / LGN2001.
- JSON output, session import and an HTTPS connectivity check.
- Standalone HTML/CSS/JavaScript documentation deployed to GitHub Pages.

### Correctness

- Recognize successful BIL1000 subscription responses with response code 00.
- Check the inner SMS result instead of relying only on the outer response code.
- Preserve sessions on failed login or refresh and bound automatic retries.
- Reuse sorting behavior between plans and search.
- Reduce request fields through endpoint-specific omission checks.

### Downloads and limitations

- Prebuilt Windows x86_64 executable provided as a ZIP archive.
- macOS and Linux users currently build from source; Linux requires an unlocked Secret Service provider.
- Paid subscriptions charge immediately. No automatic retry for network or business failures.
- Search is scoped to a single catalog tag.
- Individual SMS-header and further subscription-field omissions remain unverified.
- Local Android token-sync scripts and integration tests are excluded from the public repository.
