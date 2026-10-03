use crate::{AccountSourced, GraphicsQuality};

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

// ── exceção de uma conta não vaza para a próxima ─────────────────────────
//
// `GlobalBasicSettings_13.xml` e `ClientAppSettings.json` são globais: a
// exceção de uma conta é gravada neles logo antes de ela abrir. A conta
// seguinte, sem exceção, só reescreve o que o perfil global define — e o que o
// global não define (qualidade, volume, FPS, janela) ficava com o valor da
// exceção. O registro abaixo guarda, por propriedade, o valor que estava lá
// **antes** da exceção e o que a exceção escreveu. Na próxima abertura sem
// exceção: se o arquivo ainda tem o valor da exceção, o antigo volta; se não
// tem, o jogador mudou dentro do jogo, e o valor dele fica.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct LedgerEntry {
    /// O elemento como estava antes da primeira exceção. `None` = não existia.
    original: Option<String>,
    /// O que a última exceção escreveu.
    written: String,
}

/// O registro das exceções gravadas nos arquivos globais do Roblox. Persistido
/// em `ClientOverrideLedger.json`, na pasta de dados: sobrevive a fechar o app
/// entre a conta principal e as outras.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OverrideLedger {
    entries: std::collections::BTreeMap<String, LedgerEntry>,
}

impl OverrideLedger {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Uma propriedade depois de uma abertura. `from_account`: a exceção da
    /// conta escreveu o grupo dela; `from_global`: o perfil global escreveu.
    ///
    /// `sticky`: há cliente do Roblox aberto — o da exceção pode estar entre
    /// eles, e ele **regrava** o XML com o valor dela (a tela cheia da conta
    /// principal, 03/10/2026). Então o registro não é consumido ao desfazer:
    /// fica para a próxima conta sem exceção desfazer de novo. Só sai quando o
    /// valor no arquivo não é nem o da exceção nem o antigo (foi o jogador).
    fn settle(
        &mut self,
        key: String,
        before: Option<String>,
        after: Option<String>,
        from_account: bool,
        from_global: bool,
        sticky: bool,
    ) -> Restore {
        if from_account {
            // Grupo da exceção sem esta propriedade (qualidade automática não
            // mexe no nível manual): o registro anterior continua valendo.
            if let Some(written) = after {
                self.entries
                    .entry(key)
                    .and_modify(|e| e.written = written.clone())
                    .or_insert(LedgerEntry { original: before, written });
            }
            return Restore::Keep;
        }
        if from_global {
            // O perfil global escreveu o valor dele: é o certo para quem não
            // tem exceção, e o antigo não precisa mais voltar.
            self.entries.remove(&key);
            return Restore::Keep;
        }
        if sticky {
            let Some(entry) = self.entries.get(&key) else {
                return Restore::Keep;
            };
            if after.as_deref() == Some(entry.written.as_str()) {
                return Restore::To(entry.original.clone());
            }
            // Ainda com o valor que uma conta anterior pôs de volta.
            if after == entry.original {
                return Restore::Keep;
            }
            self.entries.remove(&key);
            return Restore::Keep;
        }
        match self.entries.remove(&key) {
            Some(entry) if after.as_deref() == Some(entry.written.as_str()) => {
                Restore::To(entry.original)
            }
            // O valor mudou desde a exceção: foi o jogador, e fica.
            _ => Restore::Keep,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Restore {
    Keep,
    To(Option<String>),
}

const FPS_PROPS: &[&str] = &["FramerateCap"];
const VOLUME_PROPS: &[&str] = &["MasterVolume"];
const GRAPHICS_PROPS: &[&str] = &[
    "GraphicsQualityLevel",
    "SavedQualityLevel",
    "QualityResetLevel",
    "MaxQualityEnabled",
];
const WINDOW_PROPS: &[&str] = &["Fullscreen", "StartMaximized", "StartScreenSize"];

/// Início e fim do elemento `<tag name="…">…</tag>` de uma propriedade.
fn property_range(props: &str, name: &str) -> Option<(usize, usize)> {
    let needle = format!(" name=\"{}\">", name);
    let at = props.find(&needle)?;
    let start = props[..at].rfind('<')?;
    let tag = &props[start + 1..at];
    if tag.is_empty() || tag.contains(|c: char| c.is_whitespace() || c == '>' || c == '/') {
        return None;
    }
    let close = format!("</{}>", tag);
    let end_rel = props[at..].find(&close)?;
    Some((start, at + end_rel + close.len()))
}

fn property_element(props: &str, name: &str) -> Option<String> {
    property_range(props, name).map(|(s, e)| props[s..e].to_string())
}

/// Põe o elemento de volta como era — ou o tira, se antes ele não existia.
fn restore_property(props: &mut String, name: &str, element: Option<&str>) {
    match (property_range(props, name), element) {
        (Some((s, e)), Some(el)) => props.replace_range(s..e, el),
        (Some((s, e)), None) => {
            // Leva junto a indentação e a quebra de linha da linha dele.
            let line_start = props[..s].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let start = if props[line_start..s].trim().is_empty() { line_start } else { s };
            let end = if props[e..].starts_with('\n') { e + 1 } else { e };
            props.replace_range(start..end, "");
        }
        (None, Some(el)) => {
            if !props.ends_with('\n') {
                props.push('\n');
            }
            props.push_str("\t\t\t");
            props.push_str(el);
            props.push('\n');
        }
        (None, None) => {}
    }
}

/// O valor de uma propriedade `<bool>`; `None` se ela não existe ou não é
/// `true`/`false`.
fn bool_property(props: &str, name: &str) -> Option<bool> {
    let element = property_element(props, name)?;
    let open = format!("<bool name=\"{}\">", name);
    let inner = element.strip_prefix(&open)?.strip_suffix("</bool>")?.trim();
    match inner {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// O XML reescrito e o que o registro decidiu sobre a tela cheia desta conta.
#[derive(Debug)]
struct GuardedRewrite {
    xml: String,
    /// Uma exceção de outra conta mexeu no `Fullscreen` e esta conta não
    /// pediu nada para a janela: o valor que ficou no arquivo é o do jogador
    /// (posto de volta pelo registro). `None` = nenhuma exceção no caminho, ou
    /// a conta/o perfil global já decidiram a janela.
    ///
    /// O launch usa isso como a tela cheia resolvida para esta conta: o cliente
    /// da exceção pode regravar `Fullscreen=true` antes de o cliente novo ler o
    /// arquivo, e é a conferência pelo PID que tira a tela cheia herdada.
    fullscreen_from_ledger: Option<bool>,
}

/// `rewrite_global_basic_settings` com o registro: grava o que a abertura pede
/// e desfaz o que uma exceção anterior deixou nas propriedades que ninguém
/// pediu desta vez. `clients_open`: há cliente do Roblox aberto (ver
/// `OverrideLedger::settle`).
#[allow(clippy::too_many_arguments)]
fn rewrite_global_basic_settings_guarded(
    xml: &str,
    max_fps: Option<u32>,
    master_volume: Option<f32>,
    graphics: Option<GraphicsQuality>,
    fullscreen: Option<bool>,
    window_size: Option<(u32, u32)>,
    from_account: AccountSourced,
    clients_open: bool,
    ledger: &mut OverrideLedger,
) -> Option<GuardedRewrite> {
    let (start, end) = find_user_game_settings_properties_range(xml)?;
    let before = xml[start..end].to_string();
    let fullscreen_key = "xml:Fullscreen".to_string();
    let exception_touched_fullscreen = ledger.entries.contains_key(&fullscreen_key);
    let written = rewrite_global_basic_settings(
        xml,
        max_fps,
        master_volume,
        graphics,
        fullscreen,
        window_size,
    )?;
    let (w_start, w_end) = find_user_game_settings_properties_range(&written)?;
    let mut props = written[w_start..w_end].to_string();

    let window_written = fullscreen.is_some() || window_size.is_some();
    let groups: [(&[&str], bool, bool); 4] = [
        (FPS_PROPS, from_account.fps, max_fps.is_some()),
        (VOLUME_PROPS, from_account.volume, master_volume.is_some()),
        (GRAPHICS_PROPS, from_account.graphics, graphics.is_some()),
        (WINDOW_PROPS, from_account.window, window_written),
    ];
    for (names, account, any_written) in groups {
        for name in names {
            let restore = ledger.settle(
                format!("xml:{}", name),
                property_element(&before, name),
                property_element(&props, name),
                account,
                any_written && !account,
                clients_open,
            );
            if let Restore::To(original) = restore {
                restore_property(&mut props, name, original.as_deref());
            }
        }
    }

    let window_decided = from_account.window || window_written;
    let fullscreen_from_ledger = if exception_touched_fullscreen && !window_decided {
        bool_property(&props, "Fullscreen")
    } else {
        None
    };

    let mut out = written[..w_start].to_string();
    out.push_str(&props);
    out.push_str(&written[w_end..]);
    Some(GuardedRewrite {
        xml: out,
        fullscreen_from_ledger,
    })
}

/// `merge_client_app_settings` com o registro, para o FPS de uma exceção não
/// ficar no `ClientAppSettings.json` da versão. `file_key` separa as pastas de
/// versão, que têm um arquivo cada.
fn merge_client_app_settings_guarded(
    settings: &mut serde_json::Value,
    max_fps: Option<u32>,
    fast_flags: Option<&serde_json::Map<String, serde_json::Value>>,
    fps_from_account: bool,
    file_key: &str,
    ledger: &mut OverrideLedger,
) {
    const FPS_KEY: &str = "DFIntTaskSchedulerTargetFps";
    let read = |v: &serde_json::Value| v.get(FPS_KEY).map(|x| x.to_string());
    let before = read(settings);
    merge_client_app_settings(settings, max_fps, fast_flags);
    let restore = ledger.settle(
        format!("json:{}:{}", file_key, FPS_KEY),
        before,
        read(settings),
        fps_from_account,
        max_fps.is_some() && !fps_from_account,
        // Cliente aberto não reescreve o `ClientAppSettings.json`.
        false,
    );
    if let Restore::To(original) = restore {
        if let Some(obj) = settings.as_object_mut() {
            match original.and_then(|raw| serde_json::from_str(&raw).ok()) {
                Some(value) => {
                    obj.insert(FPS_KEY.to_string(), value);
                }
                None => {
                    obj.remove(FPS_KEY);
                }
            }
        }
    }
}

fn override_ledger_file() -> PathBuf {
    crate::data::settings::get_runtime_data_dir().join("ClientOverrideLedger.json")
}

fn load_override_ledger() -> OverrideLedger {
    std::fs::read_to_string(override_ledger_file())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_override_ledger(ledger: &OverrideLedger) {
    let path = override_ledger_file();
    if ledger.is_empty() {
        let _ = std::fs::remove_file(path);
    } else if let Ok(raw) = serde_json::to_string_pretty(ledger) {
        let _ = std::fs::write(path, raw);
    }
}

fn apply_global_basic_settings_overrides(
    max_fps: Option<u32>,
    master_volume: Option<f32>,
    graphics: Option<GraphicsQuality>,
    fullscreen: Option<bool>,
    window_size: Option<(u32, u32)>,
    from_account: AccountSourced,
    ledger: &mut OverrideLedger,
) -> Result<Option<bool>, String> {
    let Some(path) = get_global_basic_settings_file() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let xml = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read GlobalBasicSettings_13.xml: {}", e))?;

    let Some(patched) = rewrite_global_basic_settings_guarded(
        &xml,
        max_fps,
        master_volume,
        graphics,
        fullscreen,
        window_size,
        from_account,
        !get_roblox_pids().is_empty(),
        ledger,
    ) else {
        return Ok(None);
    };
    if patched.xml != xml {
        std::fs::write(&path, &patched.xml)
            .map_err(|e| format!("Failed to write GlobalBasicSettings_13.xml: {}", e))?;
    }
    Ok(patched.fullscreen_from_ledger)
}

/// Grava as opções desta abertura nos arquivos do Roblox. `from_account` diz o
/// que veio da exceção da conta — é o que o registro guarda para desfazer na
/// próxima conta sem exceção.
///
/// Devolve a tela cheia que o registro pôs de volta para esta conta (ver
/// `GuardedRewrite::fullscreen_from_ledger`).
#[allow(clippy::too_many_arguments)]
pub fn apply_runtime_client_settings(
    base_path: Option<&str>,
    max_fps: Option<u32>,
    master_volume: Option<f32>,
    graphics: Option<GraphicsQuality>,
    fullscreen: Option<bool>,
    window_size: Option<(u32, u32)>,
    fast_flags: Option<&serde_json::Map<String, serde_json::Value>>,
    from_account: AccountSourced,
) -> Result<Option<bool>, String> {
    let mut ledger = load_override_ledger();
    let pending = !ledger.is_empty();

    if max_fps.is_some() || fast_flags.is_some() || pending {
        let settings_file = match base_path {
            Some(base) => get_client_settings_file_in(base)?,
            None => get_client_settings_file()?,
        };
        let mut settings = load_client_app_settings(&settings_file);
        let before = settings.clone();
        merge_client_app_settings_guarded(
            &mut settings,
            max_fps,
            fast_flags,
            from_account.fps,
            &settings_file.to_string_lossy(),
            &mut ledger,
        );
        if settings != before || max_fps.is_some() || fast_flags.is_some() {
            write_client_app_settings(&settings_file, &settings)?;
        }
    }

    let mut fullscreen_from_ledger = None;
    if max_fps.is_some()
        || master_volume.is_some()
        || graphics.is_some()
        || fullscreen.is_some()
        || window_size.is_some()
        || pending
    {
        fullscreen_from_ledger = apply_global_basic_settings_overrides(
            max_fps,
            master_volume,
            graphics,
            fullscreen,
            window_size,
            from_account,
            &mut ledger,
        )?;
    }

    save_override_ledger(&ledger);
    Ok(fullscreen_from_ledger)
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
