/// Pasta de dados do app dentro do perfil do usuário.
pub const APP_DIR_NAME: &str = "Roblox Account Manager";
/// Arquivo vazio ao lado do executável que força o modo portátil (dados na
/// própria pasta do exe, como nas versões antigas).
pub const PORTABLE_MARKER: &str = "portable.txt";
/// Sobrescreve a pasta de dados (usado nos testes e por quem quer pasta própria).
pub const DATA_DIR_ENV: &str = "RAM_DATA_DIR";

/// Arquivos e pastas que pertencem ao usuário e migram junto com ele.
pub const DATA_FILES: &[&str] = &[
    "AccountData.json",
    // A chave que abre o vault sem senha. Tem que viajar **junto** com o
    // `AccountData.json` na migração de pasta e no backup: um vault cifrado sem
    // a chave dele é um vault perdido. O custo é honesto e está na doc — o zip
    // de backup passa a carregar a chave, então o que protege um backup vazado
    // é o embrulho do aparelho, não o DPAPI (que só abre no perfil de origem).
    "AccountData.key",
    "RAMSettings.ini",
    "RAMTheme.ini",
    "RAMThemePresets.json",
    "RAMScripts.json",
    "RAMAvatars.json",
    // Favoritos (com os links dos servidores VIP), jogos e servidores recentes.
    // Moravam só no `localStorage` do WebView e ficavam fora do backup.
    "RAMGameLists.json",
    // Presets de launch (contas → jogo/servidor) e os horários deles.
    "RAMLaunchPresets.json",
    // Gravações (sequências de teclas, cliques e esperas) e qual vale para quem.
    "RAMRecordings.json",
    // Histórico de sessões por conta (uma linha JSON por evento, 90 dias).
    "RAMSessionHistory.jsonl",
    "AccountControlData.json",
];
pub const DATA_DIRS: &[&str] = &["RAMThemeFonts"];

/// Pasta do executável (onde as versões antigas guardavam tudo).
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// `%LOCALAPPDATA%\Roblox Account Manager` no Windows (mesma raiz que o
/// catálogo de versões e os backups de isolamento já usam), o equivalente
/// em cada outro SO, ou `None` quando a variável não existe.
pub fn app_data_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);

    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library").join("Application Support"));

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")));

    base.map(|base| base.join(APP_DIR_NAME))
}

/// Decide a pasta de dados. Separada da leitura do ambiente para poder ser
/// testada: precedência é variável de ambiente → modo portátil → perfil do
/// usuário → pasta do exe (último recurso, quando não há perfil).
fn decide_data_dir(
    env_override: Option<PathBuf>,
    portable: bool,
    app_root: Option<PathBuf>,
    exe_dir: PathBuf,
) -> PathBuf {
    if let Some(dir) = env_override.filter(|d| !d.as_os_str().is_empty()) {
        return dir;
    }
    if portable {
        return exe_dir;
    }
    app_root.unwrap_or(exe_dir)
}

/// Copia os dados do usuário de `from` para `to` **sem nunca sobrescrever** o
/// que já existe no destino e sem apagar a origem (se algo der errado, basta
/// voltar a versão antiga do app que os arquivos continuam lá).
/// Devolve o que foi migrado.
pub fn migrate_data_files(from: &Path, to: &Path) -> Vec<String> {
    let mut migrated = Vec::new();
    if from == to || !from.exists() {
        return migrated;
    }
    if std::fs::create_dir_all(to).is_err() {
        return migrated;
    }

    for name in DATA_FILES {
        let source = from.join(name);
        let target = to.join(name);
        if source.is_file() && !target.exists() && std::fs::copy(&source, &target).is_ok() {
            migrated.push((*name).to_string());
        }
    }

    for name in DATA_DIRS {
        let source = from.join(name);
        let target = to.join(name);
        if source.is_dir() && !target.exists() && copy_dir_recursive(&source, &target).is_ok() {
            migrated.push((*name).to_string());
        }
    }

    migrated
}

fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Pasta onde o app lê e grava os dados do usuário, resolvida uma vez por
/// processo (com a migração da pasta do exe feita junto, na primeira vez).
pub fn get_runtime_data_dir() -> PathBuf {
    static RESOLVED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    RESOLVED
        .get_or_init(|| {
            let exe = exe_dir();
            let dir = decide_data_dir(
                std::env::var_os(DATA_DIR_ENV).map(PathBuf::from),
                exe.join(PORTABLE_MARKER).exists(),
                app_data_root(),
                exe.clone(),
            );
            let _ = std::fs::create_dir_all(&dir);
            let migrated = migrate_data_files(&exe, &dir);
            if !migrated.is_empty() {
                eprintln!(
                    "Dados migrados da pasta do executável para {}: {}",
                    dir.display(),
                    migrated.join(", ")
                );
            }
            dir
        })
        .clone()
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

/// Mora aqui, e não em `data/scripts.rs`, porque aquele arquivo é montado
/// isolado pelo teste de integração `security_regression_scripts_store` e não
/// pode referenciar outros módulos do crate.
pub fn get_scripts_path() -> PathBuf {
    get_runtime_data_dir().join("RAMScripts.json")
}

/// Avatares salvos da aba de avatares gratuitos.
pub fn get_avatars_path() -> PathBuf {
    get_runtime_data_dir().join("RAMAvatars.json")
}

/// Favoritos, jogos recentes e servidores recentes (`data/game_lists.rs`).
pub fn get_game_lists_path() -> PathBuf {
    get_runtime_data_dir().join("RAMGameLists.json")
}

/// Presets de launch (`data/launch_presets.rs`).
pub fn get_launch_presets_path() -> PathBuf {
    get_runtime_data_dir().join("RAMLaunchPresets.json")
}

/// Gravações (`data/recordings.rs`).
pub fn get_recordings_path() -> PathBuf {
    get_runtime_data_dir().join(crate::data::recordings::RECORDINGS_FILE_NAME)
}

/// Histórico de sessões (`data/session_history.rs`).
pub fn get_session_history_path() -> PathBuf {
    get_runtime_data_dir().join(crate::data::session_history::SESSION_HISTORY_FILE_NAME)
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
