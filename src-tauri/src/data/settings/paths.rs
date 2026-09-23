fn get_runtime_data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

pub fn get_settings_path() -> PathBuf {
    get_runtime_data_dir().join("RAMSettings.ini")
}

pub fn get_theme_path() -> PathBuf {
    get_runtime_data_dir().join("RAMTheme.ini")
}

pub fn get_theme_presets_path() -> PathBuf {
    get_runtime_data_dir().join("RAMThemePresets.json")
}

pub fn get_theme_fonts_dir() -> PathBuf {
    get_runtime_data_dir().join("RAMThemeFonts")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeFontAssetImportResult {
    pub file: String,
    pub suggested_family: String,
}

fn sanitize_font_family_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Custom Font")
        .trim();
    if stem.is_empty() {
        return "Custom Font".to_string();
    }
    stem.to_string()
}

fn is_allowed_font_ext(ext: &str) -> bool {
    matches!(ext, "ttf" | "otf" | "woff" | "woff2")
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}


#[cfg(test)]
mod settings_paths_tests {
    use super::*;

    fn exe_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .expect("the test binary has a parent directory")
    }

    #[test]
    fn the_runtime_data_dir_is_the_directory_holding_the_executable() {
        let dir = get_runtime_data_dir();
        assert_eq!(dir, exe_dir());
        assert!(dir.is_absolute(), "{}", dir.display());
    }

    #[test]
    fn every_settings_path_is_a_sibling_of_the_executable() {
        let cases = [
            (get_settings_path(), "RAMSettings.ini"),
            (get_theme_path(), "RAMTheme.ini"),
            (get_theme_presets_path(), "RAMThemePresets.json"),
            (get_theme_fonts_dir(), "RAMThemeFonts"),
        ];
        for (path, expected) in cases {
            assert_eq!(path.file_name().and_then(|n| n.to_str()), Some(expected));
            assert_eq!(path.parent(), Some(exe_dir().as_path()));
        }
    }

    #[test]
    fn the_settings_paths_are_all_distinct() {
        let all = [
            get_settings_path(),
            get_theme_path(),
            get_theme_presets_path(),
            get_theme_fonts_dir(),
        ];
        let mut unique = all.to_vec();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), all.len());
    }

    // ---- font helpers -------------------------------------------------------------

    #[test]
    fn is_allowed_font_ext_accepts_only_the_four_lowercase_web_font_extensions() {
        for ext in ["ttf", "otf", "woff", "woff2"] {
            assert!(is_allowed_font_ext(ext), "{ext} should be allowed");
        }
        for ext in ["", "TTF", "WOFF2", "eot", "ttc", "json", "exe", "ttf2", ".ttf"] {
            assert!(!is_allowed_font_ext(ext), "{ext:?} should be rejected");
        }
    }

    #[test]
    fn sanitize_font_family_from_path_uses_the_trimmed_file_stem() {
        assert_eq!(
            sanitize_font_family_from_path(Path::new("C:\\fonts\\My Font.ttf")),
            "My Font"
        );
        assert_eq!(
            sanitize_font_family_from_path(Path::new("/usr/share/fonts/Inter-Regular.otf")),
            "Inter-Regular"
        );
        // Only the last extension is stripped.
        assert_eq!(
            sanitize_font_family_from_path(Path::new("Family.Bold.woff2")),
            "Family.Bold"
        );
        assert_eq!(sanitize_font_family_from_path(Path::new("NoExtension")), "NoExtension");
    }

    #[test]
    fn sanitize_font_family_from_path_falls_back_to_custom_font() {
        assert_eq!(sanitize_font_family_from_path(Path::new("")), "Custom Font");
        assert_eq!(sanitize_font_family_from_path(Path::new("   .ttf")), "Custom Font");
        assert_eq!(sanitize_font_family_from_path(Path::new("..")), "Custom Font");
    }

    #[test]
    fn to_hex_encodes_bytes_as_lowercase_pairs() {
        assert_eq!(to_hex(&[]), "");
        assert_eq!(to_hex(&[0x00]), "00");
        assert_eq!(to_hex(&[0x0f]), "0f");
        assert_eq!(to_hex(&[0xff]), "ff");
        assert_eq!(to_hex(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
        assert_eq!(to_hex(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]), "0123456789abcdef");

        let all: Vec<u8> = (0u8..=255).collect();
        let hex = to_hex(&all);
        assert_eq!(hex.len(), 512);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn the_font_import_result_serializes_its_fields_verbatim() {
        let result = ThemeFontAssetImportResult {
            file: "abc123.ttf".to_string(),
            suggested_family: "My Font".to_string(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["file"], "abc123.ttf");
        assert_eq!(
            json["suggested_family"], "My Font",
            "no rename_all: the frontend sees snake_case here"
        );
    }
}
