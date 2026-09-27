use crate::GraphicsQuality;

fn load_client_app_settings(settings_file: &std::path::Path) -> serde_json::Value {
    if settings_file.exists() {
        std::fs::read_to_string(settings_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    }
}

fn write_client_app_settings(
    settings_file: &std::path::Path,
    settings: &serde_json::Value,
) -> Result<(), String> {
    std::fs::write(
        settings_file,
        serde_json::to_string(settings).unwrap_or_default(),
    )
    .map_err(|e| format!("Failed to write ClientAppSettings.json: {}", e))
}

/// Merge the launcher-controlled values into an already loaded
/// `ClientAppSettings.json` document. Keys the app does not own are left
/// untouched so a user's hand-written flags survive a launch.
fn merge_client_app_settings(
    settings: &mut serde_json::Value,
    max_fps: Option<u32>,
    fast_flags: Option<&serde_json::Map<String, serde_json::Value>>,
) {
    // A hand-written file can hold valid JSON that is not an object (`[1,2]`,
    // `"x"`); indexing into that panics, so normalise first.
    if (max_fps.is_some() || fast_flags.is_some()) && !settings.is_object() {
        *settings = serde_json::json!({});
    }

    if let Some(fps) = max_fps {
        settings["DFIntTaskSchedulerTargetFps"] = serde_json::json!(fps);
    }

    if let Some(flags) = fast_flags {
        if let Some(target) = settings.as_object_mut() {
            for (key, value) in flags {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn apply_client_app_settings_overrides(
    base_path: Option<&str>,
    max_fps: Option<u32>,
    fast_flags: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<(), String> {
    let settings_file = match base_path {
        Some(base) => get_client_settings_file_in(base)?,
        None => get_client_settings_file()?,
    };
    let mut settings = load_client_app_settings(&settings_file);

    merge_client_app_settings(&mut settings, max_fps, fast_flags);

    write_client_app_settings(&settings_file, &settings)
}

pub fn apply_fps_unlock(max_fps: u32) -> Result<(), String> {
    apply_client_app_settings_overrides(None, Some(max_fps), None)
}

fn get_global_basic_settings_file() -> Option<PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
    Some(
        std::path::Path::new(&local_app_data)
            .join("Roblox")
            .join("GlobalBasicSettings_13.xml"),
    )
}

fn find_user_game_settings_properties_range(xml: &str) -> Option<(usize, usize)> {
    let class_pos = xml.find("class=\"UserGameSettings\"")?;
    let item_start = xml[..class_pos].rfind("<Item")?;
    let properties_open_rel = xml[item_start..].find("<Properties>")?;
    let properties_open = item_start + properties_open_rel;
    let content_start = properties_open + "<Properties>".len();
    let properties_close_rel = xml[content_start..].find("</Properties>")?;
    let content_end = content_start + properties_close_rel;
    Some((content_start, content_end))
}

fn upsert_scalar_property(props: &mut String, tag: &str, name: &str, value: &str) {
    let open = format!("<{} name=\"{}\">", tag, name);
    let close = format!("</{}>", tag);

    if let Some(start) = props.find(&open) {
        let value_start = start + open.len();
        if let Some(end_rel) = props[value_start..].find(&close) {
            let value_end = value_start + end_rel;
            props.replace_range(value_start..value_end, value);
            return;
        }
    }

    if !props.ends_with('\n') {
        props.push('\n');
    }
    props.push_str(&format!(
        "\t\t\t<{} name=\"{}\">{}</{}>\n",
        tag, name, value, tag
    ));
}

fn upsert_vector2_property(props: &mut String, name: &str, x: u32, y: u32) {
    let open = format!("<Vector2 name=\"{}\">", name);
    let close = "</Vector2>";
    let block = format!(
        "<Vector2 name=\"{}\">\n\t\t\t\t<X>{}</X>\n\t\t\t\t<Y>{}</Y>\n\t\t\t</Vector2>",
        name, x, y
    );

    if let Some(start) = props.find(&open) {
        if let Some(end_rel) = props[start..].find(close) {
            let end = start + end_rel + close.len();
            props.replace_range(start..end, &block);
            return;
        }
    }

    if !props.ends_with('\n') {
        props.push('\n');
    }
    props.push_str("\t\t\t");
    props.push_str(&block);
    props.push('\n');
}

/// Aplica as opções no XML **em memória**. Separado do arquivo de propósito: a
/// parte que escolhe o que gravar é a que precisa de teste, e o caminho real do
/// `GlobalBasicSettings_13.xml` não pode ser tocado por um teste.
///
/// `None` quando o XML não tem o bloco de `UserGameSettings` — nesse caso não há
/// nada para reescrever.
fn rewrite_global_basic_settings(
    xml: &str,
    max_fps: Option<u32>,
    master_volume: Option<f32>,
    graphics: Option<GraphicsQuality>,
    fullscreen: Option<bool>,
    window_size: Option<(u32, u32)>,
) -> Option<String> {
    let (start, end) = find_user_game_settings_properties_range(xml)?;
    let mut props = xml[start..end].to_string();

    if let Some(fps) = max_fps {
        upsert_scalar_property(&mut props, "int", "FramerateCap", &fps.to_string());
    }

    if let Some(volume) = master_volume {
        let clamped = volume.clamp(0.0, 1.0);
        upsert_scalar_property(
            &mut props,
            "float",
            "MasterVolume",
            &format!("{:.6}", clamped),
        );
    }

    match graphics {
        // `SavedQualityLevel` é o token que o Roblox lê para saber se o jogador
        // escolheu um nível ou deixou no automático; `0` é o automático. O
        // `GraphicsQualityLevel` não é tocado aqui de propósito: ele guarda o
        // último nível manual, e o cliente o ignora enquanto o token é `0`.
        Some(GraphicsQuality::Automatic) => {
            upsert_scalar_property(&mut props, "token", "SavedQualityLevel", "0");
            upsert_scalar_property(&mut props, "bool", "MaxQualityEnabled", "false");
        }
        Some(GraphicsQuality::Level(level)) => {
            let clamped = level.clamp(1, 10);
            upsert_scalar_property(
                &mut props,
                "int",
                "GraphicsQualityLevel",
                &clamped.to_string(),
            );
            upsert_scalar_property(
                &mut props,
                "token",
                "SavedQualityLevel",
                &clamped.to_string(),
            );
            upsert_scalar_property(&mut props, "int", "QualityResetLevel", &clamped.to_string());
            upsert_scalar_property(&mut props, "bool", "MaxQualityEnabled", "false");
        }
        None => {}
    }

    // Quem decide `Fullscreen` é o pedido explícito. Só quando não há pedido é
    // que um tamanho de janela implica "em janela" — é o comportamento antigo,
    // e sem ele pedir 1280x720 abriria em tela cheia do mesmo jeito.
    let janela_explicita = fullscreen == Some(false);
    if fullscreen == Some(true) {
        upsert_scalar_property(&mut props, "bool", "Fullscreen", "true");
        upsert_scalar_property(&mut props, "bool", "StartMaximized", "false");
    } else if janela_explicita || window_size.is_some() {
        upsert_scalar_property(&mut props, "bool", "Fullscreen", "false");
        upsert_scalar_property(&mut props, "bool", "StartMaximized", "false");
    }

    if let Some((w, h)) = window_size {
        let width = w.max(320);
        let height = h.max(240);
        upsert_vector2_property(&mut props, "StartScreenSize", width, height);
    }

    let mut out = xml.to_string();
    out.replace_range(start..end, &props);
    Some(out)
}

fn apply_global_basic_settings_overrides(
    max_fps: Option<u32>,
    master_volume: Option<f32>,
    graphics: Option<GraphicsQuality>,
    fullscreen: Option<bool>,
    window_size: Option<(u32, u32)>,
) -> Result<(), String> {
    let Some(path) = get_global_basic_settings_file() else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }

    let xml = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read GlobalBasicSettings_13.xml: {}", e))?;

    let Some(patched) = rewrite_global_basic_settings(
        &xml,
        max_fps,
        master_volume,
        graphics,
        fullscreen,
        window_size,
    ) else {
        return Ok(());
    };

    std::fs::write(&path, patched)
        .map_err(|e| format!("Failed to write GlobalBasicSettings_13.xml: {}", e))
}

pub fn apply_runtime_client_settings(
    base_path: Option<&str>,
    max_fps: Option<u32>,
    master_volume: Option<f32>,
    graphics: Option<GraphicsQuality>,
    fullscreen: Option<bool>,
    window_size: Option<(u32, u32)>,
    fast_flags: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<(), String> {
    if max_fps.is_some() || fast_flags.is_some() {
        apply_client_app_settings_overrides(base_path, max_fps, fast_flags)?;
    }

    if max_fps.is_some()
        || master_volume.is_some()
        || graphics.is_some()
        || fullscreen.is_some()
        || window_size.is_some()
    {
        apply_global_basic_settings_overrides(
            max_fps,
            master_volume,
            graphics,
            fullscreen,
            window_size,
        )?;
    }

    Ok(())
}

pub fn copy_custom_client_settings(
    base_path: Option<&str>,
    custom_settings_path: &str,
) -> Result<(), String> {
    let custom_path = std::path::Path::new(custom_settings_path);
    if !custom_path.exists() {
        return Err("Custom ClientAppSettings.json path does not exist".into());
    }

    let content = std::fs::read_to_string(custom_path)
        .map_err(|e| format!("Failed to read custom settings file: {}", e))?;
    serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|e| format!("Custom settings file is not valid JSON: {}", e))?;

    let settings_file = match base_path {
        Some(base) => get_client_settings_file_in(base)?,
        None => get_client_settings_file()?,
    };
    std::fs::write(settings_file, content)
        .map_err(|e| format!("Failed to copy custom ClientAppSettings.json: {}", e))
}

#[cfg(test)]
mod win_client_settings_tests {
    use super::*;

    // Every test here works on a temp file or an in-memory value. Nothing may
    // call `get_client_settings_file()`, which resolves the real Roblox install
    // folder and would write ClientAppSettings.json into it.

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("ram4-clientset-{}-{}-{}", tag, nanos, n));
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }

        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn flags(pairs: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    // ── load_client_app_settings ───────────────────────────────────────────

    #[test]
    fn load_client_app_settings_returns_an_empty_object_when_the_file_is_missing() {
        let temp = TempDir::new("loadmissing");
        let value = load_client_app_settings(&temp.file("ClientAppSettings.json"));
        assert_eq!(value, serde_json::json!({}));
    }

    #[test]
    fn load_client_app_settings_reads_an_existing_document() {
        let temp = TempDir::new("loadok");
        let path = temp.file("ClientAppSettings.json");
        std::fs::write(&path, r#"{"FFlagUserHandMadeByMe":"True","DFIntX":7}"#).unwrap();

        let value = load_client_app_settings(&path);
        assert_eq!(value["FFlagUserHandMadeByMe"], "True");
        assert_eq!(value["DFIntX"], 7);
    }

    #[test]
    fn load_client_app_settings_falls_back_to_an_empty_object_for_corrupt_json() {
        let temp = TempDir::new("loadcorrupt");
        for (name, contents) in [
            ("a.json", "{ this is not json"),
            ("b.json", ""),
            ("c.json", "\u{feff}{}"),
            ("d.json", "{\"a\": 1,}"),
        ] {
            let path = temp.file(name);
            std::fs::write(&path, contents).unwrap();
            assert_eq!(
                load_client_app_settings(&path),
                serde_json::json!({}),
                "{} should have fallen back",
                name
            );
        }
    }

    // ── write_client_app_settings ──────────────────────────────────────────

    #[test]
    fn write_client_app_settings_round_trips_through_load() {
        let temp = TempDir::new("write");
        let path = temp.file("ClientAppSettings.json");
        let settings = serde_json::json!({ "DFIntTaskSchedulerTargetFps": 240, "Keep": "me" });

        write_client_app_settings(&path, &settings).expect("write");

        assert_eq!(load_client_app_settings(&path), settings);
    }

    #[test]
    fn write_client_app_settings_reports_an_unwritable_path() {
        let temp = TempDir::new("writefail");
        // A path whose parent does not exist cannot be created implicitly.
        let path = temp.file("nope").join("ClientAppSettings.json");
        let err = write_client_app_settings(&path, &serde_json::json!({})).unwrap_err();
        assert!(
            err.starts_with("Failed to write ClientAppSettings.json"),
            "got {}",
            err
        );
    }

    // ── merge_client_app_settings ──────────────────────────────────────────

    #[test]
    fn merge_sets_the_fps_cap_under_the_task_scheduler_key() {
        let mut settings = serde_json::json!({});
        merge_client_app_settings(&mut settings, Some(240), None);
        assert_eq!(settings["DFIntTaskSchedulerTargetFps"], 240);
        assert_eq!(settings.as_object().unwrap().len(), 1);
    }

    #[test]
    fn merge_overwrites_a_previous_fps_cap() {
        let mut settings = serde_json::json!({ "DFIntTaskSchedulerTargetFps": 60 });
        merge_client_app_settings(&mut settings, Some(144), None);
        assert_eq!(settings["DFIntTaskSchedulerTargetFps"], 144);
    }

    #[test]
    fn merge_preserves_keys_the_app_does_not_own() {
        let mut settings = serde_json::json!({
            "FFlagSomethingTheUserSet": "True",
            "DFIntUserTweak": 3,
            "NestedObject": { "a": 1 },
        });
        merge_client_app_settings(
            &mut settings,
            Some(120),
            Some(&flags(&[("DFFlagDebugSkyGray", serde_json::json!("True"))])),
        );

        assert_eq!(settings["FFlagSomethingTheUserSet"], "True");
        assert_eq!(settings["DFIntUserTweak"], 3);
        assert_eq!(settings["NestedObject"]["a"], 1);
        assert_eq!(settings["DFIntTaskSchedulerTargetFps"], 120);
        assert_eq!(settings["DFFlagDebugSkyGray"], "True");
    }

    #[test]
    fn merge_lets_incoming_fast_flags_win_over_existing_values() {
        let mut settings = serde_json::json!({ "DFIntRenderShadowIntensity": 9 });
        merge_client_app_settings(
            &mut settings,
            None,
            Some(&flags(&[("DFIntRenderShadowIntensity", serde_json::json!(0))])),
        );
        assert_eq!(settings["DFIntRenderShadowIntensity"], 0);
    }

    #[test]
    fn merge_with_nothing_to_apply_leaves_the_document_untouched() {
        let original = serde_json::json!({ "Keep": "me" });
        let mut settings = original.clone();
        merge_client_app_settings(&mut settings, None, None);
        assert_eq!(settings, original);
    }

    #[test]
    fn merge_with_an_empty_flag_map_only_applies_the_fps_cap() {
        let mut settings = serde_json::json!({ "Keep": "me" });
        merge_client_app_settings(&mut settings, Some(60), Some(&flags(&[])));
        assert_eq!(settings["Keep"], "me");
        assert_eq!(settings["DFIntTaskSchedulerTargetFps"], 60);
        assert_eq!(settings.as_object().unwrap().len(), 2);
    }

    #[test]
    fn merge_replaces_a_non_object_document_before_writing_fast_flags() {
        // A ClientAppSettings.json holding a valid but non-object JSON value
        // (an array, a string, ...) is discarded rather than merged into.
        for corrupt in [
            serde_json::json!([1, 2, 3]),
            serde_json::json!("a string"),
            serde_json::json!(42),
            serde_json::Value::Null,
        ] {
            let mut settings = corrupt.clone();
            merge_client_app_settings(
                &mut settings,
                None,
                Some(&flags(&[("DFFlagDebugSkyGray", serde_json::json!("True"))])),
            );
            assert_eq!(
                settings,
                serde_json::json!({ "DFFlagDebugSkyGray": "True" }),
                "starting from {:?}",
                corrupt
            );
        }
    }

    #[test]
    fn merge_writes_the_fps_cap_into_a_null_document() {
        // serde_json turns a Null into an object on keyed assignment, so the
        // "file held null" case still produces a usable document.
        let mut settings = serde_json::Value::Null;
        merge_client_app_settings(&mut settings, Some(75), None);
        assert_eq!(settings, serde_json::json!({ "DFIntTaskSchedulerTargetFps": 75 }));
    }

    #[test]
    fn merge_result_survives_a_write_and_reload() {
        let temp = TempDir::new("mergeroundtrip");
        let path = temp.file("ClientAppSettings.json");
        std::fs::write(&path, r#"{"FFlagMine":"True"}"#).unwrap();

        let mut settings = load_client_app_settings(&path);
        merge_client_app_settings(
            &mut settings,
            Some(360),
            Some(&flags(&[("FIntDebugForceMSAASamples", serde_json::json!(0))])),
        );
        write_client_app_settings(&path, &settings).unwrap();

        let reloaded = load_client_app_settings(&path);
        assert_eq!(reloaded["FFlagMine"], "True");
        assert_eq!(reloaded["DFIntTaskSchedulerTargetFps"], 360);
        assert_eq!(reloaded["FIntDebugForceMSAASamples"], 0);
    }

    // ── GlobalBasicSettings_13.xml helpers ─────────────────────────────────

    const SAMPLE_XML: &str = concat!(
        "<roblox>\n",
        "\t<Item class=\"Other\" referent=\"RBX0\">\n",
        "\t\t<Properties>\n",
        "\t\t\t<int name=\"FramerateCap\">30</int>\n",
        "\t\t</Properties>\n",
        "\t</Item>\n",
        "\t<Item class=\"UserGameSettings\" referent=\"RBX1\">\n",
        "\t\t<Properties>\n",
        "\t\t\t<int name=\"FramerateCap\">60</int>\n",
        "\t\t\t<float name=\"MasterVolume\">0.500000</float>\n",
        "\t\t</Properties>\n",
        "\t</Item>\n",
        "</roblox>\n"
    );


    // ── rewrite_global_basic_settings: tela cheia e qualidade automática ────

    #[test]
    fn rewrite_global_basic_settings_asks_for_fullscreen() {
        let out = rewrite_global_basic_settings(SAMPLE_XML, None, None, None, Some(true), None)
            .expect("bloco UserGameSettings");
        assert!(out.contains("<bool name=\"Fullscreen\">true</bool>"));
        assert!(out.contains("<bool name=\"StartMaximized\">false</bool>"));
        // Tela cheia não carrega um tamanho de janela atrás.
        assert!(!out.contains("StartScreenSize"));
    }

    #[test]
    fn rewrite_global_basic_settings_keeps_fullscreen_when_a_size_comes_along() {
        // O tamanho continua gravado (serve para quando o jogador sair da tela
        // cheia), mas quem pediu tela cheia não pode receber Fullscreen=false.
        let out = rewrite_global_basic_settings(
            SAMPLE_XML,
            None,
            None,
            None,
            Some(true),
            Some((1920, 1080)),
        )
        .expect("bloco UserGameSettings");
        assert!(out.contains("<bool name=\"Fullscreen\">true</bool>"));
        assert!(out.contains("StartScreenSize"));
    }

    #[test]
    fn rewrite_global_basic_settings_still_windows_a_plain_size_request() {
        // Comportamento antigo: pedir um tamanho sem falar de tela cheia abre em
        // janela. Sem isto, 1280x720 abriria em tela cheia do mesmo jeito.
        let out =
            rewrite_global_basic_settings(SAMPLE_XML, None, None, None, None, Some((1280, 720)))
                .expect("bloco UserGameSettings");
        assert!(out.contains("<bool name=\"Fullscreen\">false</bool>"));
        assert!(out.contains("<bool name=\"StartMaximized\">false</bool>"));
    }

    #[test]
    fn rewrite_global_basic_settings_leaves_fullscreen_alone_when_nobody_asked() {
        let out = rewrite_global_basic_settings(SAMPLE_XML, Some(240), None, None, None, None)
            .expect("bloco UserGameSettings");
        assert!(out.contains("<int name=\"FramerateCap\">240</int>"));
        assert!(!out.contains("Fullscreen"));
        assert!(!out.contains("StartMaximized"));
    }

    #[test]
    fn rewrite_global_basic_settings_writes_automatic_quality_as_the_zero_token() {
        let out = rewrite_global_basic_settings(
            SAMPLE_XML,
            None,
            None,
            Some(GraphicsQuality::Automatic),
            None,
            None,
        )
        .expect("bloco UserGameSettings");
        assert!(out.contains("<token name=\"SavedQualityLevel\">0</token>"));
        assert!(out.contains("<bool name=\"MaxQualityEnabled\">false</bool>"));
        // O nível manual guardado não é mexido: o cliente o ignora enquanto o
        // token é 0, e sobrescrevê-lo com 1 daria a pior qualidade quando o
        // jogador voltasse para o modo manual.
        assert!(!out.contains("GraphicsQualityLevel"));
        assert!(!out.contains("QualityResetLevel"));
    }

    #[test]
    fn rewrite_global_basic_settings_writes_a_fixed_quality_level() {
        let out = rewrite_global_basic_settings(
            SAMPLE_XML,
            None,
            None,
            Some(GraphicsQuality::Level(7)),
            None,
            None,
        )
        .expect("bloco UserGameSettings");
        assert!(out.contains("<int name=\"GraphicsQualityLevel\">7</int>"));
        assert!(out.contains("<token name=\"SavedQualityLevel\">7</token>"));
        assert!(out.contains("<int name=\"QualityResetLevel\">7</int>"));
    }

    #[test]
    fn rewrite_global_basic_settings_returns_none_without_the_user_game_settings_item() {
        let xml = "<roblox>\n\t<Item class=\"Other\"><Properties></Properties></Item>\n</roblox>\n";
        assert!(rewrite_global_basic_settings(xml, Some(240), None, None, Some(true), None).is_none());
    }

    #[test]
    fn find_user_game_settings_properties_range_selects_the_right_item() {
        let (start, end) = find_user_game_settings_properties_range(SAMPLE_XML).expect("range");
        let props = &SAMPLE_XML[start..end];
        assert!(props.contains("MasterVolume"));
        assert!(props.contains("<int name=\"FramerateCap\">60</int>"));
        // The decoy Item that comes first must not be part of the range.
        assert!(!props.contains("<int name=\"FramerateCap\">30</int>"));
    }

    #[test]
    fn find_user_game_settings_properties_range_returns_none_when_the_item_is_absent() {
        assert!(find_user_game_settings_properties_range("").is_none());
        assert!(find_user_game_settings_properties_range("<roblox></roblox>").is_none());
        // Class present but no enclosing <Item>.
        assert!(find_user_game_settings_properties_range("class=\"UserGameSettings\"").is_none());
        // <Item> present but no <Properties> block.
        assert!(find_user_game_settings_properties_range(
            "<Item class=\"UserGameSettings\"></Item>"
        )
        .is_none());
    }

    #[test]
    fn upsert_scalar_property_replaces_an_existing_value_in_place() {
        let mut props = String::from("\t\t\t<int name=\"FramerateCap\">60</int>\n");
        upsert_scalar_property(&mut props, "int", "FramerateCap", "240");
        assert_eq!(props, "\t\t\t<int name=\"FramerateCap\">240</int>\n");
    }

    #[test]
    fn upsert_scalar_property_appends_when_the_property_is_missing() {
        let mut props = String::from("\t\t\t<int name=\"Other\">1</int>\n");
        upsert_scalar_property(&mut props, "bool", "Fullscreen", "false");
        assert!(props.contains("<int name=\"Other\">1</int>"));
        assert!(props.contains("<bool name=\"Fullscreen\">false</bool>"));
        assert!(props.ends_with('\n'));
    }

    #[test]
    fn upsert_scalar_property_adds_a_newline_before_appending_to_an_unterminated_block() {
        let mut props = String::from("<int name=\"Other\">1</int>");
        upsert_scalar_property(&mut props, "bool", "Fullscreen", "false");
        assert!(props.starts_with("<int name=\"Other\">1</int>\n"));
        assert!(props.contains("<bool name=\"Fullscreen\">false</bool>"));
    }

    #[test]
    fn upsert_scalar_property_only_touches_the_matching_tag_and_name() {
        let mut props = String::from(
            "\t\t\t<int name=\"FramerateCap\">60</int>\n\t\t\t<float name=\"FramerateCap\">1.0</float>\n",
        );
        upsert_scalar_property(&mut props, "float", "FramerateCap", "2.500000");
        assert!(props.contains("<int name=\"FramerateCap\">60</int>"));
        assert!(props.contains("<float name=\"FramerateCap\">2.500000</float>"));
    }

    #[test]
    fn upsert_vector2_property_replaces_an_existing_block() {
        let mut props = String::from(
            "\t\t\t<Vector2 name=\"StartScreenSize\">\n\t\t\t\t<X>800</X>\n\t\t\t\t<Y>600</Y>\n\t\t\t</Vector2>\n",
        );
        upsert_vector2_property(&mut props, "StartScreenSize", 1280, 720);
        assert!(props.contains("<X>1280</X>"));
        assert!(props.contains("<Y>720</Y>"));
        assert!(!props.contains("<X>800</X>"));
        assert_eq!(props.matches("<Vector2 name=\"StartScreenSize\">").count(), 1);
    }

    #[test]
    fn upsert_vector2_property_appends_when_missing() {
        let mut props = String::from("\t\t\t<int name=\"Other\">1</int>\n");
        upsert_vector2_property(&mut props, "StartScreenSize", 1920, 1080);
        assert!(props.contains("<Vector2 name=\"StartScreenSize\">"));
        assert!(props.contains("<X>1920</X>"));
        assert!(props.contains("<Y>1080</Y>"));
        assert!(props.contains("</Vector2>"));
    }

    // ── copy_custom_client_settings: only the branches that reject early ───

    #[test]
    fn copy_custom_client_settings_rejects_a_missing_path() {
        let temp = TempDir::new("copymissing");
        let err =
            copy_custom_client_settings(None, temp.file("nope.json").to_str().unwrap()).unwrap_err();
        assert_eq!(err, "Custom ClientAppSettings.json path does not exist");
    }

    #[test]
    fn copy_custom_client_settings_rejects_a_file_that_is_not_valid_json() {
        // Rejected before the real Roblox folder is ever resolved, so this is
        // safe to exercise in a unit test.
        let temp = TempDir::new("copybad");
        let path = temp.file("custom.json");
        std::fs::write(&path, "{ not json at all").unwrap();

        let err = copy_custom_client_settings(None, path.to_str().unwrap()).unwrap_err();
        assert!(
            err.starts_with("Custom settings file is not valid JSON"),
            "got {}",
            err
        );
    }
}
