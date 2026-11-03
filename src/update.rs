use crate::error::{Error, Result};
use std::io::{Cursor, Read};
use zip::ZipArchive;

pub const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/rohityadav-sas/ncell-cli/releases/latest";

/// Maximum allowed executable size inside a release archive (64 MB).
pub const MAX_BINARY_SIZE: u64 = 64 * 1024 * 1024;

/// Target suffix mapping based on OS and architecture.
pub fn target_asset_suffix() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("windows-x86_64.zip"),
        ("windows", "aarch64") => Some("windows-arm64.zip"),
        ("linux", "x86_64") => Some("linux-x86_64.zip"),
        ("linux", "aarch64") => Some("linux-arm64.zip"),
        ("macos", "x86_64") => Some("macos-x86_64.zip"),
        ("macos", "aarch64") => Some("macos-arm64.zip"),
        _ => None,
    }
}

/// Binary filename inside the release ZIP.
pub fn binary_name() -> &'static str {
    if cfg!(windows) { "ncell.exe" } else { "ncell" }
}

/// Parse semantic version "vX.Y.Z" or "X.Y.Z" into a semver::Version.
pub fn parse_semver(v: &str) -> Option<semver::Version> {
    let clean = v.trim().strip_prefix('v').unwrap_or(v.trim());
    semver::Version::parse(clean).ok()
}

/// Parse semantic version "vX.Y.Z" or "X.Y.Z" into a comparable tuple.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let ver = parse_semver(v)?;
    Some((ver.major, ver.minor, ver.patch))
}

/// Compare two versions. Returns true if `target` is newer than `current`.
pub fn is_newer_version(target: &str, current: &str) -> bool {
    match (parse_semver(target), parse_semver(current)) {
        (Some(t), Some(c)) => t > c,
        _ => false,
    }
}

/// Extract the executable bytes from an in-memory ZIP archive.
pub fn extract_binary_from_zip(zip_bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let reader = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(reader)
        .map_err(|e| Error::Update(format!("Invalid release ZIP archive: {e}")))?;
    let mut file = archive
        .by_name(name)
        .map_err(|_| Error::Update(format!("Executable '{name}' not found in release archive")))?;
    if file.size() > MAX_BINARY_SIZE {
        return Err(Error::Update(format!(
            "Executable '{name}' exceeds maximum allowed size ({MAX_BINARY_SIZE} bytes)"
        )));
    }
    let capacity = file.size().min(MAX_BINARY_SIZE) as usize;
    let mut buffer = Vec::with_capacity(capacity);
    file.by_ref()
        .take(MAX_BINARY_SIZE + 1)
        .read_to_end(&mut buffer)
        .map_err(|e| Error::Update(format!("Failed to read executable from archive: {e}")))?;
    if buffer.len() as u64 > MAX_BINARY_SIZE {
        return Err(Error::Update(format!(
            "Executable '{name}' exceeds maximum allowed size ({MAX_BINARY_SIZE} bytes)"
        )));
    }
    Ok(buffer)
}

/// Locate the download URL for the current platform's asset from the release JSON.
pub fn find_asset_url(release: &serde_json::Value) -> Result<(&str, &str)> {
    let suffix = target_asset_suffix().ok_or_else(|| {
        Error::Update(format!(
            "Unsupported platform: {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| Error::Update("Release response contains no assets list".into()))?;
    let asset = assets
        .iter()
        .find(|a| {
            a["name"]
                .as_str()
                .is_some_and(|name| name.ends_with(suffix))
        })
        .ok_or_else(|| {
            Error::Update(format!("No release asset found matching suffix '{suffix}'"))
        })?;
    let name = asset["name"]
        .as_str()
        .ok_or_else(|| Error::Update("Asset has invalid name".into()))?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| Error::Update("Asset is missing browser_download_url".into()))?;
    Ok((name, url))
}

/// Perform update using a custom API URL and custom binary replacer function.
pub fn update_with_replacer<F>(
    api_url: &str,
    current_version: &str,
    replacer: F,
    json: bool,
) -> Result<()>
where
    F: FnOnce(&[u8]) -> Result<()>,
{
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("ncell-cli/{current_version}"))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::Update(format!("Failed to build HTTP client: {e}")))?;

    if !json {
        println!("Checking for updates...");
    }

    let response = client
        .get(api_url)
        .send()
        .map_err(|e| Error::Update(format!("Failed to query release API: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::Update(format!("GitHub API returned HTTP {status}")));
    }

    let release: serde_json::Value = response
        .json()
        .map_err(|e| Error::Update(format!("Invalid release JSON response: {e}")))?;

    let latest_tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| Error::Update("Release JSON missing tag_name".into()))?;

    if !is_newer_version(latest_tag, current_version) {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "up_to_date",
                    "version": current_version,
                    "latest": latest_tag
                })
            );
        } else {
            println!("ncell-cli is already up to date (v{current_version}).");
        }
        return Ok(());
    }

    let (asset_name, download_url) = find_asset_url(&release)?;

    if !json {
        println!("Updating ncell-cli v{current_version} -> {latest_tag} ({asset_name})...");
    }

    let zip_bytes = client
        .get(download_url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| Error::Update(format!("Failed to download {asset_name}: {e}")))?
        .bytes()
        .map_err(|e| Error::Update(format!("Failed to read {asset_name} payload: {e}")))?;

    let binary = extract_binary_from_zip(&zip_bytes, binary_name())?;

    replacer(&binary)?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "updated",
                "previous_version": current_version,
                "version": latest_tag
            })
        );
    } else {
        println!("Successfully updated to {latest_tag}!");
    }

    Ok(())
}

/// Replace current running executable on disk with new binary bytes.
pub fn replace_current_exe(bytes: &[u8]) -> Result<()> {
    let temp_path = std::env::temp_dir().join(format!("ncell-update-{}", uuid::Uuid::new_v4()));
    std::fs::write(&temp_path, bytes)
        .map_err(|e| Error::Update(format!("Failed to write temporary binary: {e}")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755));
    }

    let result = self_replace::self_replace(&temp_path)
        .map_err(|e| Error::Update(format!("Could not replace executable: {e}")));

    let _ = std::fs::remove_file(&temp_path);
    result
}

/// Default entrypoint for auto-update command.
pub fn run(api_url_override: Option<&str>, json: bool) -> Result<()> {
    let url = api_url_override.unwrap_or(GITHUB_LATEST_RELEASE_URL);
    update_with_replacer(url, env!("CARGO_PKG_VERSION"), replace_current_exe, json)
}
