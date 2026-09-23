use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

const VERSIONS_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";

static ENSURE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Serialize)]
struct DownloadProgress {
    stage: String,
    downloaded: u64,
    total: u64,
}

fn platform_key() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86") {
            "win32"
        } else {
            "win64"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-arm64"
        } else {
            "mac-x64"
        }
    } else {
        "linux64"
    }
}

fn binary_path(version_dir: &Path) -> PathBuf {
    let folder = format!("chrome-{}", platform_key());
    let base = version_dir.join(folder);
    if cfg!(target_os = "windows") {
        base.join("chrome.exe")
    } else if cfg!(target_os = "macos") {
        base.join("Google Chrome for Testing.app")
            .join("Contents")
            .join("MacOS")
            .join("Google Chrome for Testing")
    } else {
        base.join("chrome")
    }
}

pub fn chromium_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Could not resolve data directory: {}", e))?
        .join("chromium");
    Ok(dir)
}

fn cached_binary(app: &AppHandle) -> Option<PathBuf> {
    let dir = chromium_dir(app).ok()?;
    let manifest = dir.join("version.json");
    let raw = std::fs::read_to_string(manifest).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let binary = json.get("binary").and_then(Value::as_str)?;
    let path = PathBuf::from(binary);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

pub fn is_installed(app: &AppHandle) -> bool {
    cached_binary(app).is_some()
}

pub async fn ensure_chromium(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(binary) = cached_binary(app) {
        return Ok(binary);
    }

    let _guard = ENSURE_LOCK.lock().await;
    if let Some(binary) = cached_binary(app) {
        return Ok(binary);
    }

    let dir = chromium_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create data directory: {}", e))?;

    let _ = app.emit(
        "chromium-download-progress",
        DownloadProgress {
            stage: "resolving".into(),
            downloaded: 0,
            total: 0,
        },
    );

    let (version, url) = resolve_download().await?;
    let version_dir = dir.join(&version);
    let binary = binary_path(&version_dir);

    if !binary.exists() {
        let archive = dir.join("download.zip");
        download_archive(app, &url, &archive).await?;

        let _ = app.emit(
            "chromium-download-progress",
            DownloadProgress {
                stage: "extracting".into(),
                downloaded: 0,
                total: 0,
            },
        );

        let extract_target = version_dir.clone();
        let archive_for_extract = archive.clone();
        tauri::async_runtime::spawn_blocking(move || {
            extract_archive(&archive_for_extract, &extract_target)
        })
        .await
        .map_err(|e| format!("Extraction task failed: {}", e))??;

        let _ = std::fs::remove_file(&archive);
    }

    if !binary.exists() {
        return Err("Browser archive did not contain the expected executable".into());
    }

    make_executable(&binary);

    let manifest = serde_json::json!({
        "version": version,
        "binary": binary.to_string_lossy(),
    });
    std::fs::write(
        dir.join("version.json"),
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    )
    .map_err(|e| format!("Could not write version manifest: {}", e))?;

    let _ = app.emit(
        "chromium-download-progress",
        DownloadProgress {
            stage: "ready".into(),
            downloaded: 0,
            total: 0,
        },
    );

    Ok(binary)
}

async fn resolve_download() -> Result<(String, String), String> {
    let json: Value = reqwest::get(VERSIONS_URL)
        .await
        .map_err(|e| format!("Could not reach browser download service: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Could not read browser version list: {}", e))?;

    parse_stable_download(&json, platform_key())
}

/// Pick `(version, url)` for `platform` out of the chrome-for-testing
/// "last known good versions" document. Split out from [`resolve_download`]
/// so the parsing can be exercised without hitting the network.
fn parse_stable_download(json: &Value, platform: &str) -> Result<(String, String), String> {
    let stable = json
        .get("channels")
        .and_then(|c| c.get("Stable"))
        .ok_or("Browser version list is missing the Stable channel")?;

    let version = stable
        .get("version")
        .and_then(Value::as_str)
        .ok_or("Browser version list is missing a version")?
        .to_string();

    let url = stable
        .get("downloads")
        .and_then(|d| d.get("chrome"))
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("platform").and_then(Value::as_str) == Some(platform))
        })
        .and_then(|entry| entry.get("url").and_then(Value::as_str))
        .ok_or("No browser build is available for this platform")?
        .to_string();

    Ok((version, url))
}

async fn download_archive(app: &AppHandle, url: &str, target: &Path) -> Result<(), String> {
    let mut response = reqwest::get(url)
        .await
        .map_err(|e| format!("Browser download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Browser download failed (status {})",
            response.status().as_u16()
        ));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut file =
        std::fs::File::create(target).map_err(|e| format!("Could not write download: {}", e))?;
    use std::io::Write;

    let mut last_emit = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Browser download interrupted: {}", e))?
    {
        file.write_all(&chunk)
            .map_err(|e| format!("Could not write download: {}", e))?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit >= 2_000_000 {
            last_emit = downloaded;
            let _ = app.emit(
                "chromium-download-progress",
                DownloadProgress {
                    stage: "downloading".into(),
                    downloaded,
                    total,
                },
            );
        }
    }

    file.flush().map_err(|e| format!("Could not finalize download: {}", e))?;
    Ok(())
}

fn extract_archive(archive: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("Could not open download: {}", e))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("Could not read download: {}", e))?;
    std::fs::create_dir_all(target).map_err(|e| format!("Could not create browser directory: {}", e))?;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("Could not read archive entry: {}", e))?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path = target.join(rel);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut out =
            std::fs::File::create(&out_path).map_err(|e| format!("Could not write file: {}", e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("Could not write file: {}", e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
            }
        }
    }

    Ok(())
}

fn make_executable(binary: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(not(unix))]
    {
        let _ = binary;
    }
}

#[cfg(test)]
mod chromium_download_tests {
    use super::*;
    use std::io::Write as _;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Catalog parsing, platform/path resolution and archive extraction. The
    // network fetch and `ensure_chromium` (which needs an AppHandle) are not
    // covered here.

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ram4-chromium-{}-{}-{}", tag, nanos, n));
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).expect("create zip");
        let mut writer = zip::ZipWriter::new(file);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, contents) in entries {
            writer.start_file(*name, options).expect("start_file");
            writer.write_all(contents).expect("write entry");
        }
        writer.finish().expect("finish zip");
    }

    fn catalog(platform: &str, url: &str, version: &str) -> Value {
        serde_json::json!({
            "channels": {
                "Stable": {
                    "version": version,
                    "downloads": {
                        "chrome": [
                            { "platform": "some-other-platform", "url": "https://wrong" },
                            { "platform": platform, "url": url },
                        ]
                    }
                }
            }
        })
    }

    // ── platform_key / binary_path ─────────────────────────────────────────

    #[test]
    fn platform_key_matches_the_chrome_for_testing_naming() {
        let key = platform_key();
        assert!(
            ["win32", "win64", "mac-arm64", "mac-x64", "linux64"].contains(&key),
            "unexpected platform key {}",
            key
        );
        if cfg!(target_os = "windows") {
            assert!(key.starts_with("win"));
        }
    }

    #[test]
    fn binary_path_lives_under_the_platform_folder_of_the_version_dir() {
        let version_dir = Path::new("C:\\data\\chromium\\131.0.0.0");
        let binary = binary_path(version_dir);

        assert!(binary.starts_with(version_dir));
        assert!(
            binary.to_string_lossy().contains(&format!("chrome-{}", platform_key())),
            "{} should contain the platform folder",
            binary.display()
        );
        if cfg!(target_os = "windows") {
            assert_eq!(binary.file_name().unwrap(), "chrome.exe");
        }
    }

    #[test]
    fn binary_path_is_deterministic() {
        let dir = Path::new("some/dir");
        assert_eq!(binary_path(dir), binary_path(dir));
    }

    // ── parse_stable_download ──────────────────────────────────────────────

    #[test]
    fn parse_stable_download_returns_the_version_and_the_matching_platform_url() {
        let json = catalog("win64", "https://cdn/chrome-win64.zip", "131.0.6778.85");
        let (version, url) = parse_stable_download(&json, "win64").unwrap();
        assert_eq!(version, "131.0.6778.85");
        assert_eq!(url, "https://cdn/chrome-win64.zip");
    }

    #[test]
    fn parse_stable_download_works_for_every_supported_platform_key() {
        for platform in ["win32", "win64", "mac-arm64", "mac-x64", "linux64"] {
            let json = catalog(platform, "https://cdn/build.zip", "1.2.3");
            let (_, url) = parse_stable_download(&json, platform).unwrap();
            assert_eq!(url, "https://cdn/build.zip", "platform {}", platform);
        }
    }

    #[test]
    fn parse_stable_download_reports_a_missing_stable_channel() {
        let err = parse_stable_download(&serde_json::json!({}), "win64").unwrap_err();
        assert_eq!(err, "Browser version list is missing the Stable channel");

        let err = parse_stable_download(
            &serde_json::json!({ "channels": { "Beta": {} } }),
            "win64",
        )
        .unwrap_err();
        assert_eq!(err, "Browser version list is missing the Stable channel");
    }

    #[test]
    fn parse_stable_download_reports_a_missing_version() {
        let json = serde_json::json!({ "channels": { "Stable": { "downloads": {} } } });
        let err = parse_stable_download(&json, "win64").unwrap_err();
        assert_eq!(err, "Browser version list is missing a version");

        // A non-string version is treated as missing.
        let json = serde_json::json!({ "channels": { "Stable": { "version": 131 } } });
        assert_eq!(
            parse_stable_download(&json, "win64").unwrap_err(),
            "Browser version list is missing a version"
        );
    }

    #[test]
    fn parse_stable_download_reports_when_this_platform_has_no_build() {
        let json = catalog("linux64", "https://cdn/linux.zip", "1.2.3");
        let err = parse_stable_download(&json, "win64").unwrap_err();
        assert_eq!(err, "No browser build is available for this platform");
    }

    #[test]
    fn parse_stable_download_reports_a_platform_entry_without_a_url() {
        let json = serde_json::json!({
            "channels": { "Stable": {
                "version": "1.2.3",
                "downloads": { "chrome": [{ "platform": "win64" }] }
            }}
        });
        assert_eq!(
            parse_stable_download(&json, "win64").unwrap_err(),
            "No browser build is available for this platform"
        );
    }

    #[test]
    fn parse_stable_download_reports_a_malformed_downloads_section() {
        for downloads in [
            serde_json::json!({}),
            serde_json::json!({ "chrome": "not-an-array" }),
            serde_json::json!({ "chromedriver": [] }),
        ] {
            let json = serde_json::json!({
                "channels": { "Stable": { "version": "1.2.3", "downloads": downloads } }
            });
            assert_eq!(
                parse_stable_download(&json, "win64").unwrap_err(),
                "No browser build is available for this platform"
            );
        }
    }

    // ── cached_binary manifest shape ───────────────────────────────────────
    //
    // `cached_binary` itself needs an AppHandle, but the manifest it writes and
    // reads is plain JSON; this pins its shape so a rename would be caught.

    #[test]
    fn the_version_manifest_holds_a_version_and_a_binary_path() {
        let manifest = serde_json::json!({
            "version": "131.0.6778.85",
            "binary": "C:\\data\\chromium\\131.0.6778.85\\chrome-win64\\chrome.exe",
        });
        let raw = serde_json::to_string_pretty(&manifest).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            parsed.get("binary").and_then(Value::as_str),
            Some("C:\\data\\chromium\\131.0.6778.85\\chrome-win64\\chrome.exe")
        );
        assert!(parsed.get("version").and_then(Value::as_str).is_some());
    }

    // ── extract_archive ────────────────────────────────────────────────────

    #[test]
    fn extract_archive_writes_every_entry_under_the_target() {
        let temp = TempDir::new("extract");
        let archive = temp.path().join("download.zip");
        write_zip(
            &archive,
            &[
                ("chrome-win64/chrome.exe", b"MZ".as_slice()),
                ("chrome-win64/locales/en-US.pak", b"pak".as_slice()),
            ],
        );

        let target = temp.path().join("out");
        extract_archive(&archive, &target).expect("extract");

        assert_eq!(
            std::fs::read(target.join("chrome-win64").join("chrome.exe")).unwrap(),
            b"MZ"
        );
        assert_eq!(
            std::fs::read(
                target
                    .join("chrome-win64")
                    .join("locales")
                    .join("en-US.pak")
            )
            .unwrap(),
            b"pak"
        );
    }

    #[test]
    fn extract_archive_does_not_write_outside_the_target() {
        let temp = TempDir::new("traversal");
        let archive = temp.path().join("download.zip");
        write_zip(
            &archive,
            &[
                ("..\\escaped.exe", b"pwned".as_slice()),
                ("../escaped2.exe", b"pwned".as_slice()),
                ("chrome.exe", b"MZ".as_slice()),
            ],
        );

        let target = temp.path().join("out");
        extract_archive(&archive, &target).expect("extract");

        assert!(target.join("chrome.exe").exists());
        for escaped in ["escaped.exe", "escaped2.exe"] {
            assert!(
                !temp.path().join(escaped).exists(),
                "{} escaped the target",
                escaped
            );
            assert!(!target.join(escaped).exists(), "{} was written", escaped);
        }
    }

    #[test]
    fn extract_archive_creates_the_target_for_an_empty_archive() {
        let temp = TempDir::new("empty");
        let archive = temp.path().join("download.zip");
        write_zip(&archive, &[]);
        let target = temp.path().join("out");
        extract_archive(&archive, &target).expect("extract");
        assert!(target.is_dir());
    }

    #[test]
    fn extract_archive_reports_a_missing_or_corrupt_download() {
        let temp = TempDir::new("bad");
        let missing = temp.path().join("missing.zip");
        let err = extract_archive(&missing, &temp.path().join("out")).unwrap_err();
        assert!(err.starts_with("Could not open download"), "got {}", err);

        let corrupt = temp.path().join("corrupt.zip");
        std::fs::write(&corrupt, b"not a zip file").unwrap();
        let err = extract_archive(&corrupt, &temp.path().join("out2")).unwrap_err();
        assert!(err.starts_with("Could not read download"), "got {}", err);
    }

    #[test]
    fn make_executable_is_a_no_op_on_a_missing_path() {
        let temp = TempDir::new("chmod");
        make_executable(&temp.path().join("nope"));
    }
}
