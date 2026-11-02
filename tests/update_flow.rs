use mockito::Server;
use ncell_cli::error::Error;
use ncell_cli::update::{
    binary_name, extract_binary_from_zip, is_newer_version, parse_version, target_asset_suffix,
    update_with_replacer,
};
use serde_json::json;
use std::io::{Cursor, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use zip::ZipWriter;
use zip::write::FileOptions;

fn make_test_zip(filename: &str, content: &[u8]) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(&mut buffer);
    let options = FileOptions::<()>::default();
    writer.start_file(filename, options).unwrap();
    writer.write_all(content).unwrap();
    writer.finish().unwrap();
    buffer.into_inner()
}

#[test]
fn version_parsing_and_comparison_handles_semver() {
    assert_eq!(parse_version("v0.5.4"), Some((0, 5, 4)));
    assert_eq!(parse_version("0.5.4"), Some((0, 5, 4)));
    assert_eq!(parse_version("1.0.0"), Some((1, 0, 0)));
    assert_eq!(parse_version("invalid"), None);

    assert!(is_newer_version("v0.5.5", "0.5.4"));
    assert!(is_newer_version("v1.0.0", "0.5.4"));
    assert!(is_newer_version("0.6.0", "0.5.9"));
    assert!(!is_newer_version("v0.5.4", "0.5.4"));
    assert!(!is_newer_version("v0.5.3", "0.5.4"));
    assert!(!is_newer_version("0.5.4", "0.5.4"));
}

#[test]
fn target_asset_suffix_is_defined_for_current_platform() {
    let suffix = target_asset_suffix();
    assert!(suffix.is_some());
    assert!(suffix.unwrap().ends_with(".zip"));
}

#[test]
fn extracts_binary_from_valid_zip() {
    let bin_name = binary_name();
    let expected = b"MOCK_EXECUTABLE_BYTES";
    let zip_bytes = make_test_zip(bin_name, expected);

    let extracted = extract_binary_from_zip(&zip_bytes, bin_name).unwrap();
    assert_eq!(extracted, expected);
}

#[test]
fn extraction_fails_on_corrupt_zip() {
    let result = extract_binary_from_zip(b"not-a-valid-zip-header", "ncell");
    assert!(matches!(result, Err(Error::Update(_))));
}

#[test]
fn extraction_fails_when_binary_is_missing_in_zip() {
    let zip_bytes = make_test_zip("other_file.txt", b"hello");
    let result = extract_binary_from_zip(&zip_bytes, "ncell");
    assert!(matches!(result, Err(Error::Update(_))));
}

#[test]
fn update_reports_already_up_to_date_without_downloading() {
    let mut server = Server::new();
    let release_body = json!({
        "tag_name": "v0.5.4",
        "assets": []
    });

    let mock = server
        .mock("GET", "/releases/latest")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(release_body.to_string())
        .create();

    let replacer_called = Arc::new(AtomicBool::new(false));
    let replacer_called_clone = replacer_called.clone();

    let replacer = move |_bytes: &[u8]| {
        replacer_called_clone.store(true, Ordering::SeqCst);
        Ok(())
    };

    let api_url = format!("{}/releases/latest", server.url());
    let result = update_with_replacer(&api_url, "0.5.4", replacer, false);

    assert!(result.is_ok());
    assert!(!replacer_called.load(Ordering::SeqCst));
    mock.assert();
}

#[test]
fn update_downloads_extracts_and_replaces_when_newer_version_available() {
    let mut server = Server::new();
    let suffix = target_asset_suffix().expect("supported platform in tests");
    let asset_name = format!("ncell-cli-v0.6.0-{suffix}");
    let download_path = format!("/download/{asset_name}");
    let download_url = format!("{}{download_path}", server.url());

    let release_body = json!({
        "tag_name": "v0.6.0",
        "assets": [
            {
                "name": asset_name,
                "browser_download_url": download_url
            }
        ]
    });

    let mock_api = server
        .mock("GET", "/releases/latest")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(release_body.to_string())
        .create();

    let bin_name = binary_name();
    let expected_bin = b"NEW_NCELL_BINARY_V0.6.0";
    let zip_bytes = make_test_zip(bin_name, expected_bin);

    let mock_download = server
        .mock("GET", download_path.as_str())
        .with_status(200)
        .with_header("content-type", "application/zip")
        .with_body(zip_bytes)
        .create();

    let replaced_bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let replaced_bytes_clone = replaced_bytes.clone();

    let replacer = move |bytes: &[u8]| {
        let mut guard = replaced_bytes_clone.lock().unwrap();
        guard.extend_from_slice(bytes);
        Ok(())
    };

    let api_url = format!("{}/releases/latest", server.url());
    let result = update_with_replacer(&api_url, "0.5.4", replacer, false);

    assert!(result.is_ok());
    assert_eq!(*replaced_bytes.lock().unwrap(), expected_bin);
    mock_api.assert();
    mock_download.assert();
}

#[test]
fn update_fails_gracefully_on_http_error() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/releases/latest")
        .with_status(500)
        .create();

    let api_url = format!("{}/releases/latest", server.url());
    let result = update_with_replacer(&api_url, "0.5.4", |_| Ok(()), false);

    assert!(matches!(result, Err(Error::Update(_))));
    mock.assert();
}

#[test]
fn update_fails_when_matching_platform_asset_is_missing() {
    let mut server = Server::new();
    let release_body = json!({
        "tag_name": "v0.6.0",
        "assets": [
            {
                "name": "ncell-cli-v0.6.0-some-unrelated-os.zip",
                "browser_download_url": "https://example.com/download.zip"
            }
        ]
    });

    let mock = server
        .mock("GET", "/releases/latest")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(release_body.to_string())
        .create();

    let api_url = format!("{}/releases/latest", server.url());
    let result = update_with_replacer(&api_url, "0.5.4", |_| Ok(()), false);

    assert!(matches!(result, Err(Error::Update(_))));
    mock.assert();
}
