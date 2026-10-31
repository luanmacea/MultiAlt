// Sober (Roblox para Android no Linux, Flatpak `org.vinegarhq.Sober`): a parte
// PURA do launch. Nada aqui chama `Command` nem depende do SO, então os testes
// rodam no Windows com `cargo test`; quem executa de verdade é `platform/linux/`.
//
// Desenho (estudo em docs/features/linux.md, só leitura):
// - uma pasta por conta, usada como `HOME` do Sober: o Flatpak dá a cada conta
//   o seu `~/.var/app/org.vinegarhq.Sober`, com login e dados próprios;
// - o cookie é um arquivo (`.ROBLOSECURITY=<valor>`), então escrever o que o
//   MultiAlt já guarda entra na conta sem login no cliente;
// - várias contas ao mesmo tempo: cada cliente roda em `unshare --user --net`
//   e ganha internet pelo `pasta` (pacote `passt`);
// - entra no jogo por deep link `roblox://experiences/start?...`.
//
// Segurança de caminho: este módulo só escreve/apaga DENTRO da pasta de perfis
// (`<dados>/linux-profiles`), recusa symlink em qualquer ponto do caminho e do
// perfil, e do HOME real do usuário só LÊ (cópia do Sober principal).

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Id Flatpak do Sober.
pub const SOBER_APP_ID: &str = "org.vinegarhq.Sober";
/// Pasta (dentro dos dados do app) que guarda um perfil por conta.
pub const PROFILES_DIR_NAME: &str = "linux-profiles";
/// Valor padrão de `[Linux] PreferredRunner`.
pub const RUNNER_SOBER: &str = "sober";
/// `[Linux] PreferredRunner = custom` usa `[Linux] CustomLaunchCommand`.
pub const RUNNER_CUSTOM: &str = "custom";
/// Valor padrão de `[Linux] CustomProcessMatch`.
pub const DEFAULT_PROCESS_MATCH: &str = "sober,flatpak,roblox,robloxplayerbeta";
/// Nome do cookie do Roblox no arquivo do Sober.
pub const COOKIE_NAME: &str = ".ROBLOSECURITY";
/// Quanto o script do cliente espera pela rota padrão antes de seguir mesmo
/// assim (regra nº 1: nunca travar o jogar por uma etapa auxiliar).
pub const ROUTE_WAIT_TENTHS: u32 = 150;

/// Roda dentro do `unshare`: espera a rota padrão aparecer em `/proc/net/route`
/// (o `pasta --config-net` cria a interface e a rota depois que o processo já
/// está no netns) e então troca para o cliente. Recebe o comando em `"$@"`, sem
/// reinterpretar nada: cada argumento chega inteiro, sem aspas para escapar.
pub const WAIT_FOR_ROUTE_SCRIPT: &str = "i=0; while [ \"$i\" -lt 150 ]; do while read -r _ dest _; do [ \"$dest\" = 00000000 ] && break 2; done < /proc/net/route; i=$((i+1)); sleep 0.1; done; exec \"$@\"";

// ---------------------------------------------------------------------------
// Caminhos
// ---------------------------------------------------------------------------

/// `<dados>/linux-profiles`.
pub fn profiles_root(data_dir: &Path) -> PathBuf {
    data_dir.join(PROFILES_DIR_NAME)
}

/// `<dados>/linux-profiles/<userId>`. O id vem de conta salva e é sempre
/// positivo; qualquer outra coisa é recusada (nunca vira `..` ou pasta vazia).
pub fn profile_dir(data_dir: &Path, user_id: i64) -> Result<PathBuf, String> {
    if user_id <= 0 {
        return Err(format!("Invalid account id for a Linux profile: {user_id}"));
    }
    Ok(profiles_root(data_dir).join(user_id.to_string()))
}

/// Onde o Sober guarda as coisas dentro de um `HOME` (o do perfil ou o real).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoberPaths {
    pub home: PathBuf,
    /// `<home>/.var/app/org.vinegarhq.Sober`
    pub app_dir: PathBuf,
    /// `<app>/data/sober`
    pub data_dir: PathBuf,
    /// `<app>/data/sober/cookies`
    pub cookies_file: PathBuf,
    /// `<app>/config/sober/config.json`
    pub config_file: PathBuf,
    /// `<app>/data/sober/sober_logs` (onde o Sober escreve o log do cliente).
    pub log_dir: PathBuf,
}

pub fn sober_paths(home: &Path) -> SoberPaths {
    let app_dir = home.join(".var").join("app").join(SOBER_APP_ID);
    let data_dir = app_dir.join("data").join("sober");
    SoberPaths {
        home: home.to_path_buf(),
        cookies_file: data_dir.join("cookies"),
        config_file: app_dir.join("config").join("sober").join("config.json"),
        log_dir: data_dir.join("sober_logs"),
        data_dir,
        app_dir,
    }
}

/// O que um perfil novo copia do Sober principal, relativo a `.var/app/<id>`.
/// Cópia de verdade (hardlink trava com "another instance is already
/// modifying"). **Nunca** inclui `cookies`: o login do Sober principal não
/// pode vazar para a conta do perfil (o teste trava isso).
pub const COPY_ENTRIES: &[&str] = &[
    "data/sober/packages",
    "data/sober/assets",
    "data/sober/exe",
    "data/sober/asset_overlay",
    "data/sober/state",
    "config/sober/config.json",
];

/// Pares (origem no Sober principal, destino no perfil).
pub fn copy_plan(main: &SoberPaths, profile: &SoberPaths) -> Vec<(PathBuf, PathBuf)> {
    COPY_ENTRIES
        .iter()
        .map(|rel| {
            let rel_path: PathBuf = rel.split('/').collect();
            (main.app_dir.join(&rel_path), profile.app_dir.join(rel_path))
        })
        .collect()
}

/// Pasta da instalação do usuário no Flatpak: `FLATPAK_USER_DIR`, senão
/// `$XDG_DATA_HOME/flatpak`, senão `~/.local/share/flatpak`.
pub fn flatpak_user_dir(
    flatpak_user_dir_env: Option<&Path>,
    xdg_data_home: Option<&Path>,
    home: &Path,
) -> PathBuf {
    if let Some(dir) = flatpak_user_dir_env.filter(|d| !d.as_os_str().is_empty()) {
        return dir.to_path_buf();
    }
    let data_home = xdg_data_home
        .filter(|d| !d.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".local").join("share"));
    data_home.join("flatpak")
}

/// Onde o Sober pode estar instalado (usuário e sistema).
pub fn sober_install_candidates(flatpak_user_dir: &Path) -> Vec<PathBuf> {
    vec![
        flatpak_user_dir.join("app").join(SOBER_APP_ID),
        PathBuf::from("/var/lib/flatpak/app").join(SOBER_APP_ID),
    ]
}

// ---------------------------------------------------------------------------
// Avisos
// ---------------------------------------------------------------------------

/// Aviso do launch que não impede o jogo (regra nº 1). É um tipo, e não um
/// texto, para o Console traduzir cada um (`commands/launch_linux.rs`);
/// `render` é o inglês, para log e teste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoberWarning {
    /// `PreferredRunner = custom` sem `CustomLaunchCommand`.
    CustomCommandEmpty,
    /// `CustomLaunchCommand` com aspas abertas.
    CustomCommandInvalid(String),
    /// `PreferredRunner` com um nome que ainda não existe.
    UnknownRunner(String),
    /// Servidor privado sem código de link: abre o jogo comum.
    PrivateServerWithoutCode,
    /// Symlinks do Sober principal que não foram copiados.
    SkippedSymlinks(usize),
    /// Aberto sem rede própria (falta o `pasta`/`unshare`, ou eles falharam):
    /// vale para UMA conta; a segunda ao mesmo tempo precisa do `passt`.
    NetworkIsolationUnavailable,
}

impl SoberWarning {
    pub fn render(&self) -> String {
        match self {
            Self::CustomCommandEmpty => {
                "PreferredRunner is custom but CustomLaunchCommand is empty; using Sober".to_string()
            }
            Self::CustomCommandInvalid(e) => {
                format!("CustomLaunchCommand is invalid ({e}); using Sober")
            }
            Self::UnknownRunner(name) => {
                format!("Runner \"{name}\" is not supported on Linux yet; using Sober")
            }
            Self::PrivateServerWithoutCode => {
                "Private server without a link code; opening the experience instead".to_string()
            }
            Self::SkippedSymlinks(n) => {
                format!("{n} symbolic link(s) in the main Sober folder were not copied")
            }
            Self::NetworkIsolationUnavailable => {
                "Opening this account without its own network; a second account at the same time needs the passt package".to_string()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Runner (settings `[Linux]`)
// ---------------------------------------------------------------------------

/// As quatro chaves de `[Linux]` que o launch lê.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunnerSettings {
    pub preferred_runner: String,
    pub custom_launch_command: String,
    pub custom_process_match: String,
    pub custom_log_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runner {
    /// Sober em Flatpak, com perfil por conta e rede isolada.
    Sober,
    /// Comando do usuário (`CustomLaunchCommand`), já separado em argumentos.
    Custom(CommandSpec),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerPlan {
    pub runner: Runner,
    /// Pedaços do `CustomProcessMatch` (minúsculos, sem vazios).
    pub process_match: Vec<String>,
    /// `CustomLogDir`, quando preenchido.
    pub log_dir: Option<PathBuf>,
    /// Avisos para o log do launch (nunca erro: a regra nº 1 manda seguir).
    pub warnings: Vec<SoberWarning>,
}

/// Decide o runner. `custom` sem comando, ou um nome que ainda não existe
/// (Cordial), cai no Sober com um aviso em vez de recusar o launch.
pub fn plan_runner(settings: &RunnerSettings) -> RunnerPlan {
    let mut warnings = Vec::new();
    let preferred = settings.preferred_runner.trim().to_ascii_lowercase();
    let preferred = if preferred.is_empty() { RUNNER_SOBER.to_string() } else { preferred };

    let runner = match preferred.as_str() {
        RUNNER_SOBER => Runner::Sober,
        RUNNER_CUSTOM => match split_command(&settings.custom_launch_command) {
            Ok(spec) if !spec.program.is_empty() => Runner::Custom(spec),
            Ok(_) => {
                warnings.push(SoberWarning::CustomCommandEmpty);
                Runner::Sober
            }
            Err(e) => {
                warnings.push(SoberWarning::CustomCommandInvalid(e));
                Runner::Sober
            }
        },
        other => {
            warnings.push(SoberWarning::UnknownRunner(other.to_string()));
            Runner::Sober
        }
    };

    let log_dir = {
        let trimmed = settings.custom_log_dir.trim();
        (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
    };

    RunnerPlan {
        runner,
        process_match: parse_process_match(&settings.custom_process_match),
        log_dir,
        warnings,
    }
}

/// `"sober, Flatpak,,roblox"` -> `["sober", "flatpak", "roblox"]`. Vazio usa o
/// padrão do INI.
pub fn parse_process_match(raw: &str) -> Vec<String> {
    let parsed: Vec<String> = raw
        .split(',')
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    if parsed.is_empty() && raw.trim().is_empty() {
        return parse_process_match(DEFAULT_PROCESS_MATCH);
    }
    parsed
}

/// O nome do processo (`comm`) ou a linha de comando bate com algum pedaço?
pub fn process_matches(haystack: &str, patterns: &[String]) -> bool {
    let lower = haystack.to_ascii_lowercase();
    patterns.iter().any(|p| !p.is_empty() && lower.contains(p.as_str()))
}

/// Pasta de logs do cliente: a do INI quando preenchida, senão a do Sober do perfil.
pub fn client_log_dir(plan: &RunnerPlan, profile: &SoberPaths) -> PathBuf {
    plan.log_dir.clone().unwrap_or_else(|| profile.log_dir.clone())
}

// ---------------------------------------------------------------------------
// Cookie
// ---------------------------------------------------------------------------

/// Conteúdo do arquivo `cookies` do Sober: `.ROBLOSECURITY=<valor>` (sem
/// quebra de linha). Aceita o valor puro ou já com o prefixo. Recusa vazio e
/// qualquer espaço, `;` ou caractere de controle (não é cookie, e uma quebra de
/// linha viraria outra entrada no arquivo).
pub fn cookie_file_content(cookie: &str) -> Result<String, String> {
    let trimmed = cookie.trim();
    let value = trimmed
        .strip_prefix(COOKIE_NAME)
        .and_then(|rest| rest.strip_prefix('='))
        .unwrap_or(trimmed);
    if value.is_empty() {
        return Err("The saved cookie is empty".to_string());
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control() || c == ';') {
        return Err("The saved cookie has spaces, line breaks or ';' in it".to_string());
    }
    Ok(format!("{COOKIE_NAME}={value}"))
}

// ---------------------------------------------------------------------------
// Deep link
// ---------------------------------------------------------------------------

/// O jogo a abrir. Os campos chegam já resolvidos (`resolve_launch_job` em
/// `launch_shared.rs`: tira o prefixo `vip:`, lê o código do link privado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLink {
    pub place_id: i64,
    /// Servidor público específico (`gameInstanceId`); vazio = qualquer.
    pub job_id: String,
    /// Servidor privado (`linkCode`); vazio = não é privado.
    pub link_code: String,
    pub launch_data: String,
    /// Coisas que foram ignoradas por não darem para atender.
    pub warnings: Vec<SoberWarning>,
}

impl DeepLink {
    pub fn from_launch(
        place_id: i64,
        job_id: &str,
        join_vip: bool,
        link_code: &str,
        launch_data: &str,
    ) -> Result<Self, String> {
        if place_id <= 0 {
            return Err("Missing place id for the launch".to_string());
        }
        let mut warnings = Vec::new();
        let job_id = job_id.trim();
        let link_code = link_code.trim();

        let (job_id, link_code) = if join_vip && !link_code.is_empty() {
            // Servidor privado se endereça pelo código; o job id (quando vem)
            // não tem o que fazer junto.
            (String::new(), link_code.to_string())
        } else {
            if join_vip {
                warnings.push(SoberWarning::PrivateServerWithoutCode);
            }
            (job_id.to_string(), String::new())
        };

        Ok(Self {
            place_id,
            job_id,
            link_code,
            launch_data: launch_data.trim().to_string(),
            warnings,
        })
    }

    /// `roblox://experiences/start?placeId=P[&gameInstanceId=J|&linkCode=C][&launchData=D]`
    pub fn uri(&self) -> String {
        let mut uri = format!("roblox://experiences/start?placeId={}", self.place_id);
        if !self.link_code.is_empty() {
            uri.push_str("&linkCode=");
            uri.push_str(&urlencoding::encode(&self.link_code));
        } else if !self.job_id.is_empty() {
            uri.push_str("&gameInstanceId=");
            uri.push_str(&urlencoding::encode(&self.job_id));
        }
        if !self.launch_data.is_empty() {
            uri.push_str("&launchData=");
            uri.push_str(&urlencoding::encode(&self.launch_data));
        }
        uri
    }
}

// ---------------------------------------------------------------------------
// Comandos (argv)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }

    /// Programa + argumentos numa lista só (o que o `sh -c` recebe em `"$@"`).
    pub fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }
}

/// `flatpak run --branch=stable --arch=x86_64 --command=sober org.vinegarhq.Sober -- <uri>`
pub fn sober_command(uri: &str) -> CommandSpec {
    CommandSpec::new(
        "flatpak",
        &[
            "run",
            "--branch=stable",
            "--arch=x86_64",
            "--command=sober",
            SOBER_APP_ID,
            "--",
            uri,
        ],
    )
}

/// Embrulha o comando num netns próprio (`unshare --user --map-current-user
/// --net`). Sem `--fork`, o `unshare` vira o `sh` e o `sh` vira o cliente por
/// `exec`: o pid que o app guarda é o mesmo do começo ao fim, e é nele que o
/// `pasta` se conecta.
pub fn isolated_command(inner: &CommandSpec) -> CommandSpec {
    let mut args: Vec<String> = [
        "--user",
        "--map-current-user",
        "--net",
        "--",
        "sh",
        "-c",
        WAIT_FOR_ROUTE_SCRIPT,
        "sh",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.extend(inner.argv());
    CommandSpec { program: "unshare".to_string(), args }
}

/// `pasta --config-net <pid>`: liga o netns do cliente à internet do host.
pub fn pasta_command(pid: u32) -> CommandSpec {
    CommandSpec {
        program: "pasta".to_string(),
        args: vec!["--config-net".to_string(), pid.to_string()],
    }
}

/// O comando completo de uma conta no Sober: `unshare ... flatpak run ... <uri>`.
pub fn sober_launch_command(link: &DeepLink) -> CommandSpec {
    isolated_command(&sober_command(&link.uri()))
}

/// O comando do runner `custom`: troca `{uri}`/`{url}`, `{profile}` e `{user}`
/// nos argumentos; sem `{uri}`/`{url}` em lugar nenhum, o link vai no fim.
pub fn custom_launch_command(
    template: &CommandSpec,
    uri: &str,
    profile_dir: &Path,
    user_id: i64,
) -> CommandSpec {
    let profile = profile_dir.to_string_lossy().into_owned();
    let user = user_id.to_string();
    let has_uri = template
        .argv()
        .iter()
        .any(|a| a.contains("{uri}") || a.contains("{url}"));
    let fill = |s: &str| {
        s.replace("{uri}", uri)
            .replace("{url}", uri)
            .replace("{profile}", &profile)
            .replace("{user}", &user)
    };
    let mut args: Vec<String> = template.args.iter().map(|a| fill(a)).collect();
    if !has_uri {
        args.push(uri.to_string());
    }
    CommandSpec { program: fill(&template.program), args }
}

/// Separa uma linha de comando em argumentos (sem shell): espaços separam,
/// `'...'` é literal, `"..."` aceita `\"` e `\\`, `\x` fora de aspas escapa.
pub fn split_command(line: &str) -> Result<CommandSpec, String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_token {
                    tokens.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            '\'' => {
                in_token = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => return Err("unclosed single quote".to_string()),
                    }
                }
            }
            '"' => {
                in_token = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.peek().copied() {
                            Some(n @ ('"' | '\\')) => {
                                cur.push(n);
                                chars.next();
                            }
                            _ => cur.push('\\'),
                        },
                        Some(ch) => cur.push(ch),
                        None => return Err("unclosed double quote".to_string()),
                    }
                }
            }
            '\\' => {
                in_token = true;
                match chars.next() {
                    Some(n) => cur.push(n),
                    None => cur.push('\\'),
                }
            }
            other => {
                in_token = true;
                cur.push(other);
            }
        }
    }
    if in_token {
        tokens.push(cur);
    }
    let mut iter = tokens.into_iter();
    let program = iter.next().unwrap_or_default();
    Ok(CommandSpec { program, args: iter.collect() })
}

// ---------------------------------------------------------------------------
// Ambiente do processo
// ---------------------------------------------------------------------------

/// O que o app enxerga do ambiente do usuário.
#[derive(Debug, Clone, Default)]
pub struct HostEnv {
    pub home: PathBuf,
    pub xdg_data_home: Option<PathBuf>,
    pub flatpak_user_dir: Option<PathBuf>,
    pub xauthority: Option<PathBuf>,
    /// `<home>/.Xauthority` existe?
    pub default_xauthority_exists: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvPlan {
    pub set: Vec<(String, String)>,
    pub remove: Vec<String>,
}

impl EnvPlan {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.set.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// Ambiente do cliente Sober de uma conta. `HOME` aponta para o perfil; como o
/// `flatpak` acha a instalação do usuário e o `.Xauthority` a partir do `HOME`,
/// os dois caminhos reais são fixados à parte (senão o perfil não enxergaria o
/// Sober instalado nem abriria janela no X11).
pub fn sober_env_plan(host: &HostEnv, profile_home: &Path, user_id: i64) -> EnvPlan {
    let user_dir = flatpak_user_dir(
        host.flatpak_user_dir.as_deref(),
        host.xdg_data_home.as_deref(),
        &host.home,
    );
    let mut set = vec![
        ("HOME".to_string(), profile_home.to_string_lossy().into_owned()),
        ("FLATPAK_USER_DIR".to_string(), user_dir.to_string_lossy().into_owned()),
        ("MULTIALT_USER_ID".to_string(), user_id.to_string()),
    ];
    if host.xauthority.is_none() && host.default_xauthority_exists {
        set.push((
            "XAUTHORITY".to_string(),
            host.home.join(".Xauthority").to_string_lossy().into_owned(),
        ));
    }
    EnvPlan {
        set,
        remove: ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

/// Ambiente do runner `custom`: o `HOME` real fica como está; o comando recebe
/// o id da conta e a pasta do perfil por variável.
pub fn custom_env_plan(profile_home: &Path, user_id: i64) -> EnvPlan {
    EnvPlan {
        set: vec![
            ("MULTIALT_USER_ID".to_string(), user_id.to_string()),
            (
                "MULTIALT_PROFILE_DIR".to_string(),
                profile_home.to_string_lossy().into_owned(),
            ),
        ],
        remove: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Segurança de caminho e arquivos
// ---------------------------------------------------------------------------

fn check_not_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(format!(
            "Refusing to use a symbolic link inside the Linux profiles: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Cannot inspect {}: {e}", path.display())),
    }
}

/// `path` fica dentro de `root` (sem `..`) e nenhum pedaço dele, de `root` para
/// baixo, é symlink. O que ainda não existe passa.
pub fn ensure_safe_path(root: &Path, path: &Path) -> Result<(), String> {
    let rel = path.strip_prefix(root).map_err(|_| {
        format!("{} is outside the profiles folder {}", path.display(), root.display())
    })?;
    let mut current = root.to_path_buf();
    check_not_symlink(&current)?;
    for component in rel.components() {
        match component {
            Component::Normal(part) => {
                current.push(part);
                check_not_symlink(&current)?;
            }
            _ => {
                return Err(format!("Unsafe path component in {}", path.display()));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyStats {
    pub files: usize,
    /// Symlinks do Sober principal que ficaram de fora (nunca seguidos).
    pub skipped_symlinks: usize,
}

/// Copia `src` (arquivo ou pasta) para `dst`. Symlinks da origem são pulados.
pub fn copy_tree(src: &Path, dst: &Path, stats: &mut CopyStats) -> Result<(), String> {
    let meta = fs::symlink_metadata(src).map_err(|e| format!("Cannot read {}: {e}", src.display()))?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        stats.skipped_symlinks += 1;
        return Ok(());
    }
    if kind.is_dir() {
        fs::create_dir_all(dst).map_err(|e| format!("Cannot create {}: {e}", dst.display()))?;
        let entries = fs::read_dir(src).map_err(|e| format!("Cannot list {}: {e}", src.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Cannot list {}: {e}", src.display()))?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()), stats)?;
        }
        return Ok(());
    }
    if kind.is_file() {
        fs::copy(src, dst).map_err(|e| format!("Cannot copy {}: {e}", src.display()))?;
        stats.files += 1;
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileReport {
    pub home: PathBuf,
    /// A pasta do perfil foi criada agora.
    pub created: bool,
    /// Entradas copiadas do Sober principal agora (relativas a `.var/app/...`).
    pub copied: Vec<String>,
    /// Entradas que o Sober principal não tem (normal se ele nunca abriu).
    pub missing_in_main: Vec<String>,
    pub skipped_symlinks: usize,
}

/// Cria o perfil da conta (ou completa um que já existe): só copia o que o
/// perfil ainda não tem, então o estado próprio da conta nunca é sobrescrito.
/// Copia para `<destino>.partial` e renomeia, para uma cópia interrompida não
/// passar por pronta. `main_home` é o HOME real do usuário: só leitura.
pub fn ensure_profile(
    data_dir: &Path,
    user_id: i64,
    main_home: &Path,
) -> Result<ProfileReport, String> {
    let root = profiles_root(data_dir);
    let home = profile_dir(data_dir, user_id)?;

    fs::create_dir_all(&root).map_err(|e| format!("Cannot create {}: {e}", root.display()))?;
    ensure_safe_path(&root, &home)?;
    let created = !home.exists();
    fs::create_dir_all(&home).map_err(|e| format!("Cannot create {}: {e}", home.display()))?;

    let profile = sober_paths(&home);
    let main = sober_paths(main_home);
    if main.app_dir.starts_with(&root) {
        return Err("The main Sober folder cannot be inside the Linux profiles".to_string());
    }

    let mut report = ProfileReport { home: home.clone(), created, ..Default::default() };
    for (rel, (src, dst)) in COPY_ENTRIES.iter().zip(copy_plan(&main, &profile)) {
        ensure_safe_path(&root, &dst)?;
        if fs::symlink_metadata(&dst).is_ok() {
            continue;
        }
        if fs::symlink_metadata(&src).is_err() {
            report.missing_in_main.push((*rel).to_string());
            continue;
        }
        let parent = dst.parent().ok_or_else(|| "Profile path has no parent".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;

        let partial = partial_path(&dst);
        ensure_safe_path(&root, &partial)?;
        remove_inside(&root, &partial)?;
        let mut stats = CopyStats::default();
        if let Err(e) = copy_tree(&src, &partial, &mut stats) {
            let _ = remove_inside(&root, &partial);
            return Err(e);
        }
        fs::rename(&partial, &dst)
            .map_err(|e| format!("Cannot finish copying {}: {e}", dst.display()))?;
        report.skipped_symlinks += stats.skipped_symlinks;
        report.copied.push((*rel).to_string());
    }
    Ok(report)
}

fn partial_path(dst: &Path) -> PathBuf {
    let mut name = dst.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".partial");
    dst.with_file_name(name)
}

/// Apaga `target` (arquivo ou pasta), mas só se ele ficar dentro de `root`,
/// diferente do próprio `root`, e sem symlink no caminho. É a única função
/// deste módulo que apaga.
pub fn remove_inside(root: &Path, target: &Path) -> Result<(), String> {
    if target == root {
        return Err("Refusing to remove the profiles folder itself".to_string());
    }
    ensure_safe_path(root, target)?;
    let meta = match fs::symlink_metadata(target) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("Cannot inspect {}: {e}", target.display())),
    };
    let result = if meta.is_dir() { fs::remove_dir_all(target) } else { fs::remove_file(target) };
    result.map_err(|e| format!("Cannot remove {}: {e}", target.display()))
}

/// Apaga o perfil de uma conta (pasta inteira).
pub fn remove_profile(data_dir: &Path, user_id: i64) -> Result<(), String> {
    let root = profiles_root(data_dir);
    let home = profile_dir(data_dir, user_id)?;
    remove_inside(&root, &home)
}

/// Escreve o cookie no arquivo do Sober do perfil (permissão 0600 já na
/// criação), por arquivo temporário + rename.
pub fn write_cookie(data_dir: &Path, user_id: i64, cookie: &str) -> Result<PathBuf, String> {
    let content = cookie_file_content(cookie)?;
    let root = profiles_root(data_dir);
    let home = profile_dir(data_dir, user_id)?;
    let paths = sober_paths(&home);
    ensure_safe_path(&root, &paths.cookies_file)?;

    let dir = &paths.data_dir;
    fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    ensure_safe_path(&root, dir)?;

    let tmp = dir.join("cookies.tmp");
    ensure_safe_path(&root, &tmp)?;
    remove_inside(&root, &tmp)?;

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&tmp)
        .map_err(|e| format!("Cannot write the cookie file: {e}"))?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("Cannot write the cookie file: {e}"))?;
    drop(file);
    fs::rename(&tmp, &paths.cookies_file)
        .map_err(|e| format!("Cannot write the cookie file: {e}"))?;
    Ok(paths.cookies_file)
}

// ---------------------------------------------------------------------------
// /proc: processos de uma conta
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcStat {
    pub state: char,
    pub ppid: u32,
}

/// Lê `/proc/<pid>/stat`: `pid (comm) S ppid ...`. O `comm` pode ter espaços e
/// parênteses, então o corte é no ÚLTIMO `)`.
pub fn parse_proc_stat(content: &str) -> Option<ProcStat> {
    let after = &content[content.rfind(')')? + 1..];
    let mut fields = after.split_whitespace();
    let state = fields.next()?.chars().next()?;
    let ppid = fields.next()?.parse().ok()?;
    Some(ProcStat { state, ppid })
}

/// Zumbi e morto contam como "não está mais rodando".
pub fn is_running_state(state: char) -> bool {
    !matches!(state, 'Z' | 'X' | 'x')
}

/// `root` e todos os descendentes, a partir de pares (pid, ppid).
pub fn process_tree(table: &[(u32, u32)], root: u32) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(pid, ppid) in table {
        children.entry(ppid).or_default().push(pid);
    }
    let mut seen: HashSet<u32> = HashSet::from([root]);
    let mut order = vec![root];
    let mut index = 0;
    while index < order.len() {
        let current = order[index];
        index += 1;
        if let Some(kids) = children.get(&current) {
            for &kid in kids {
                if seen.insert(kid) {
                    order.push(kid);
                }
            }
        }
    }
    order
}

/// O alvo de `/proc/<pid>/ns/net` (`net:[4026531840]`) vira o id do netns.
pub fn parse_netns_id(link_target: &str) -> Option<u64> {
    link_target
        .strip_prefix("net:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

// ---------------------------------------------------------------------------
// Diagnóstico
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    pub flatpak: Option<PathBuf>,
    pub sober_installed: bool,
    pub unshare: Option<PathBuf>,
    pub pasta: Option<PathBuf>,
}

/// O que falta para abrir uma conta. Tipo, e não texto, para o Console
/// traduzir (`commands/launch_linux.rs`); `render` é o inglês.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupIssue {
    FlatpakMissing,
    SoberMissing,
    UnshareMissing,
    PastaMissing,
}

/// Como o cliente de uma conta é aberto quanto à rede.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkPlan {
    /// `unshare` + `pasta`: netns próprio, várias contas juntas.
    Isolated,
    /// `flatpak run` direto, na rede do host. Só uma conta assim por vez
    /// (o Sober recusa a segunda na mesma rede: "frozen instance").
    Plain(SoberWarning),
    /// Não dá: já há outro cliente aberto e a rede própria não está disponível.
    /// `Some` = o que falta instalar; `None` = a tentativa isolada falhou.
    Refuse(Option<SetupIssue>),
}

/// Regra nº 1: sem `pasta`/`unshare` (ou com a tentativa isolada falhando) a
/// conta ainda abre, desde que nenhum outro cliente do app esteja aberto.
/// `other_clients` = clientes do app abertos de OUTRAS contas.
pub fn plan_network(
    unshare_ok: bool,
    pasta_ok: bool,
    other_clients: usize,
    isolated_attempt_failed: bool,
) -> NetworkPlan {
    if !isolated_attempt_failed && unshare_ok && pasta_ok {
        return NetworkPlan::Isolated;
    }
    if other_clients == 0 {
        return NetworkPlan::Plain(SoberWarning::NetworkIsolationUnavailable);
    }
    let issue = if !unshare_ok {
        Some(SetupIssue::UnshareMissing)
    } else if !pasta_ok {
        Some(SetupIssue::PastaMissing)
    } else {
        None
    };
    NetworkPlan::Refuse(issue)
}

impl SetupIssue {
    pub fn render(self) -> String {
        match self {
            Self::FlatpakMissing => "Flatpak was not found. Install it from your distribution (package \"flatpak\").".to_string(),
            Self::SoberMissing => format!("Sober is not installed. Run: flatpak install flathub {SOBER_APP_ID}"),
            Self::UnshareMissing => "\"unshare\" was not found (package \"util-linux\"); it is needed to run more than one account.".to_string(),
            Self::PastaMissing => "\"pasta\" was not found. Install the \"passt\" package; it gives each account its own network so several can run at once.".to_string(),
        }
    }
}

impl Diagnostics {
    pub fn issues(&self) -> Vec<SetupIssue> {
        let mut out = Vec::new();
        if self.flatpak.is_none() {
            out.push(SetupIssue::FlatpakMissing);
        } else if !self.sober_installed {
            out.push(SetupIssue::SoberMissing);
        }
        if self.unshare.is_none() {
            out.push(SetupIssue::UnshareMissing);
        }
        if self.pasta.is_none() {
            out.push(SetupIssue::PastaMissing);
        }
        out
    }

    /// Mensagens (em inglês) para diagnóstico. Vazio = tudo no lugar.
    pub fn problems(&self) -> Vec<String> {
        self.issues().into_iter().map(SetupIssue::render).collect()
    }
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod sober_tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "multialt-sober-{label}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    // --- caminhos ---

    #[test]
    fn the_profile_lives_under_linux_profiles_by_user_id() {
        let dir = profile_dir(Path::new("/data"), 42).unwrap();
        assert_eq!(dir, Path::new("/data").join("linux-profiles").join("42"));
    }

    #[test]
    fn a_profile_needs_a_positive_user_id() {
        assert!(profile_dir(Path::new("/data"), 0).is_err());
        assert!(profile_dir(Path::new("/data"), -1).is_err());
    }

    #[test]
    fn the_sober_paths_follow_the_flatpak_layout() {
        let paths = sober_paths(Path::new("/p"));
        let app = Path::new("/p/.var/app/org.vinegarhq.Sober");
        assert_eq!(paths.app_dir, app);
        assert_eq!(paths.cookies_file, app.join("data/sober/cookies"));
        assert_eq!(paths.config_file, app.join("config/sober/config.json"));
        assert_eq!(paths.data_dir, app.join("data/sober"));
    }

    #[test]
    fn the_flatpak_user_dir_prefers_the_env_then_xdg_then_home() {
        let home = Path::new("/home/u");
        assert_eq!(
            flatpak_user_dir(Some(Path::new("/x/fp")), Some(Path::new("/y")), home),
            Path::new("/x/fp")
        );
        assert_eq!(
            flatpak_user_dir(None, Some(Path::new("/y")), home),
            Path::new("/y/flatpak")
        );
        assert_eq!(
            flatpak_user_dir(None, None, home),
            Path::new("/home/u/.local/share/flatpak")
        );
        assert_eq!(
            flatpak_user_dir(Some(Path::new("")), None, home),
            Path::new("/home/u/.local/share/flatpak")
        );
    }

    #[test]
    fn the_sober_install_is_looked_for_per_user_and_system_wide() {
        let candidates = sober_install_candidates(Path::new("/u/flatpak"));
        assert!(candidates.contains(&PathBuf::from("/u/flatpak/app/org.vinegarhq.Sober")));
        assert!(candidates.contains(&PathBuf::from("/var/lib/flatpak/app/org.vinegarhq.Sober")));
    }

    // --- copia ---

    #[test]
    fn the_copy_list_has_what_a_new_profile_needs() {
        assert_eq!(
            COPY_ENTRIES,
            [
                "data/sober/packages",
                "data/sober/assets",
                "data/sober/exe",
                "data/sober/asset_overlay",
                "data/sober/state",
                "config/sober/config.json",
            ]
        );
    }

    #[test]
    fn the_main_sober_login_is_never_copied() {
        assert!(COPY_ENTRIES.iter().all(|e| !e.contains("cookies")));
    }

    #[test]
    fn the_copy_plan_maps_main_to_profile_one_to_one() {
        let main = sober_paths(Path::new("/main"));
        let profile = sober_paths(Path::new("/prof"));
        let plan = copy_plan(&main, &profile);
        assert_eq!(plan.len(), COPY_ENTRIES.len());
        assert_eq!(
            plan[0],
            (
                PathBuf::from("/main/.var/app/org.vinegarhq.Sober/data/sober/packages"),
                PathBuf::from("/prof/.var/app/org.vinegarhq.Sober/data/sober/packages"),
            )
        );
        assert_eq!(
            plan[5].1,
            Path::new("/prof/.var/app/org.vinegarhq.Sober/config/sober/config.json")
        );
    }

    #[test]
    fn a_new_profile_copies_the_main_sober_for_real() {
        let tmp = TempDir::new("copy");
        let main_home = tmp.path().join("main");
        let main = sober_paths(&main_home);
        write(&main.data_dir.join("packages/a/b.bin"), "pkg");
        write(&main.data_dir.join("exe/run"), "exe");
        write(&main.data_dir.join("state"), "st");
        write(&main.data_dir.join("cookies"), ".ROBLOSECURITY=main-account");
        write(&main.config_file, "{}");
        let data = tmp.path().join("data");

        let report = ensure_profile(&data, 7, &main_home).unwrap();

        assert!(report.created);
        assert_eq!(report.home, profile_dir(&data, 7).unwrap());
        let profile = sober_paths(&report.home);
        assert_eq!(fs::read_to_string(profile.data_dir.join("packages/a/b.bin")).unwrap(), "pkg");
        assert_eq!(fs::read_to_string(profile.data_dir.join("state")).unwrap(), "st");
        assert_eq!(fs::read_to_string(&profile.config_file).unwrap(), "{}");
        // O login do Sober principal fica de fora.
        assert!(!profile.cookies_file.exists());
        assert!(report.missing_in_main.contains(&"data/sober/assets".to_string()));
        assert!(report.copied.contains(&"data/sober/exe".to_string()));
        // Nada de sobra do `.partial`.
        assert!(!profile.data_dir.join("exe.partial").exists());
    }

    #[test]
    fn an_existing_profile_keeps_its_own_state() {
        let tmp = TempDir::new("keep");
        let main_home = tmp.path().join("main");
        write(&sober_paths(&main_home).data_dir.join("state"), "from-main");
        let data = tmp.path().join("data");
        let home = profile_dir(&data, 7).unwrap();
        write(&sober_paths(&home).data_dir.join("state"), "from-profile");

        let report = ensure_profile(&data, 7, &main_home).unwrap();

        assert!(!report.created);
        assert!(!report.copied.contains(&"data/sober/state".to_string()));
        assert_eq!(
            fs::read_to_string(sober_paths(&home).data_dir.join("state")).unwrap(),
            "from-profile"
        );
    }

    #[test]
    fn a_stale_partial_copy_is_replaced() {
        let tmp = TempDir::new("partial");
        let main_home = tmp.path().join("main");
        write(&sober_paths(&main_home).data_dir.join("exe/run"), "good");
        let data = tmp.path().join("data");
        let home = profile_dir(&data, 3).unwrap();
        write(&sober_paths(&home).data_dir.join("exe.partial/junk"), "half");

        ensure_profile(&data, 3, &main_home).unwrap();

        let exe = sober_paths(&home).data_dir.join("exe");
        assert_eq!(fs::read_to_string(exe.join("run")).unwrap(), "good");
        assert!(!exe.join("junk").exists());
    }

    #[test]
    fn a_profile_without_a_main_sober_is_created_empty() {
        let tmp = TempDir::new("nomain");
        let report = ensure_profile(&tmp.path().join("data"), 9, &tmp.path().join("nothing")).unwrap();
        assert!(report.created);
        assert!(report.copied.is_empty());
        assert_eq!(report.missing_in_main.len(), COPY_ENTRIES.len());
    }

    #[test]
    fn the_main_sober_cannot_be_inside_the_profiles() {
        let tmp = TempDir::new("inside");
        let data = tmp.path().join("data");
        let nested_home = profile_dir(&data, 1).unwrap();
        assert!(ensure_profile(&data, 2, &nested_home).is_err());
    }

    // --- seguranca de caminho ---

    #[test]
    fn a_path_outside_the_root_is_refused() {
        let root = Path::new("/data/linux-profiles");
        assert!(ensure_safe_path(root, Path::new("/data/other")).is_err());
        assert!(ensure_safe_path(root, Path::new("/data/linux-profiles/../x")).is_err());
        assert!(ensure_safe_path(root, Path::new("/data/linux-profiles/1/a")).is_ok());
    }

    #[test]
    fn nothing_is_removed_outside_the_profiles_or_the_root_itself() {
        let tmp = TempDir::new("rm");
        let data = tmp.path().join("data");
        let root = profiles_root(&data);
        fs::create_dir_all(&root).unwrap();
        let outside = tmp.path().join("precious.txt");
        write(&outside, "keep");

        assert!(remove_inside(&root, &outside).is_err());
        assert!(remove_inside(&root, &root).is_err());
        assert!(remove_inside(&root, &root.join("..").join("precious.txt")).is_err());
        assert!(outside.exists());
    }

    #[test]
    fn removing_a_profile_removes_only_that_account() {
        let tmp = TempDir::new("rmprof");
        let data = tmp.path().join("data");
        for id in [1, 2] {
            write(&profile_dir(&data, id).unwrap().join("file"), "x");
        }
        remove_profile(&data, 1).unwrap();
        assert!(!profile_dir(&data, 1).unwrap().exists());
        assert!(profile_dir(&data, 2).unwrap().join("file").exists());
        // Remover de novo (ou um perfil que nunca existiu) não é erro.
        remove_profile(&data, 1).unwrap();
        assert!(remove_profile(&data, 0).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_inside_the_profile_is_refused() {
        use std::os::unix::fs::symlink;
        let tmp = TempDir::new("symlink");
        let data = tmp.path().join("data");
        let elsewhere = tmp.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        let home = profile_dir(&data, 5).unwrap();
        fs::create_dir_all(home.join(".var/app")).unwrap();
        symlink(&elsewhere, home.join(".var/app/org.vinegarhq.Sober")).unwrap();

        assert!(write_cookie(&data, 5, "abc").is_err());
        assert!(ensure_profile(&data, 5, &tmp.path().join("main")).is_err());
        assert!(remove_inside(&profiles_root(&data), &home.join(".var/app/org.vinegarhq.Sober")).is_err());
        assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_profile_folder_is_refused() {
        use std::os::unix::fs::symlink;
        let tmp = TempDir::new("symlink-home");
        let data = tmp.path().join("data");
        let elsewhere = tmp.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        fs::create_dir_all(profiles_root(&data)).unwrap();
        symlink(&elsewhere, profile_dir(&data, 6).unwrap()).unwrap();
        assert!(ensure_profile(&data, 6, &tmp.path().join("main")).is_err());
        assert!(write_cookie(&data, 6, "abc").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_in_the_main_sober_are_skipped_not_followed() {
        use std::os::unix::fs::symlink;
        let tmp = TempDir::new("srclink");
        let main_home = tmp.path().join("main");
        let main = sober_paths(&main_home);
        let secret = tmp.path().join("secret.txt");
        write(&secret, "secret");
        write(&main.data_dir.join("exe/run"), "x");
        symlink(&secret, main.data_dir.join("exe/link")).unwrap();

        let data = tmp.path().join("data");
        let report = ensure_profile(&data, 4, &main_home).unwrap();

        assert_eq!(report.skipped_symlinks, 1);
        assert!(!sober_paths(&report.home).data_dir.join("exe/link").exists());
    }

    // --- cookie ---

    #[test]
    fn the_cookie_file_is_the_name_equals_value() {
        assert_eq!(cookie_file_content("abc123").unwrap(), ".ROBLOSECURITY=abc123");
        assert_eq!(
            cookie_file_content("  _|WARNING:-DO-NOT-SHARE|_ABCDEF\n").unwrap(),
            ".ROBLOSECURITY=_|WARNING:-DO-NOT-SHARE|_ABCDEF"
        );
    }

    #[test]
    fn a_cookie_that_already_has_the_name_is_not_doubled() {
        assert_eq!(
            cookie_file_content(".ROBLOSECURITY=abc123").unwrap(),
            ".ROBLOSECURITY=abc123"
        );
    }

    #[test]
    fn a_broken_cookie_is_refused() {
        assert!(cookie_file_content("").is_err());
        assert!(cookie_file_content("   ").is_err());
        assert!(cookie_file_content(".ROBLOSECURITY=").is_err());
        assert!(cookie_file_content("abc def").is_err());
        assert!(cookie_file_content("abc\nother=1").is_err());
        assert!(cookie_file_content("abc;def").is_err());
    }

    #[test]
    fn the_cookie_is_written_inside_the_profile() {
        let tmp = TempDir::new("cookie");
        let data = tmp.path().join("data");
        let path = write_cookie(&data, 11, "value-1").unwrap();
        assert_eq!(
            path,
            sober_paths(&profile_dir(&data, 11).unwrap()).cookies_file
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), ".ROBLOSECURITY=value-1");
        // Trocar o cookie substitui o arquivo, sem sobra do temporário.
        write_cookie(&data, 11, "value-2").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), ".ROBLOSECURITY=value-2");
        assert!(!path.with_file_name("cookies.tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_cookie_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new("cookie-mode");
        let path = write_cookie(tmp.path(), 1, "v").unwrap();
        assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    // --- deep link ---

    #[test]
    fn a_public_launch_opens_the_experience() {
        let link = DeepLink::from_launch(606849621, "", false, "", "").unwrap();
        assert_eq!(link.uri(), "roblox://experiences/start?placeId=606849621");
    }

    #[test]
    fn a_job_id_adds_the_game_instance() {
        let link = DeepLink::from_launch(1, " 0f3a-bb12 ", false, "", "").unwrap();
        assert_eq!(link.uri(), "roblox://experiences/start?placeId=1&gameInstanceId=0f3a-bb12");
    }

    #[test]
    fn a_private_server_uses_the_link_code() {
        let link = DeepLink::from_launch(606849621, "", true, "8811223344", "").unwrap();
        assert_eq!(
            link.uri(),
            "roblox://experiences/start?placeId=606849621&linkCode=8811223344"
        );
    }

    #[test]
    fn a_private_server_ignores_the_job_id() {
        let link = DeepLink::from_launch(5, "some-job", true, "code", "").unwrap();
        assert_eq!(link.uri(), "roblox://experiences/start?placeId=5&linkCode=code");
    }

    #[test]
    fn a_private_server_without_a_code_falls_back_with_a_warning() {
        let link = DeepLink::from_launch(5, "job", true, " ", "").unwrap();
        assert_eq!(link.uri(), "roblox://experiences/start?placeId=5&gameInstanceId=job");
        assert_eq!(link.warnings.len(), 1);
    }

    #[test]
    fn launch_data_and_odd_characters_are_encoded() {
        let link = DeepLink::from_launch(5, "", true, "a b&c", "x=1&y=2").unwrap();
        assert_eq!(
            link.uri(),
            "roblox://experiences/start?placeId=5&linkCode=a%20b%26c&launchData=x%3D1%26y%3D2"
        );
    }

    #[test]
    fn a_launch_without_a_place_is_refused() {
        assert!(DeepLink::from_launch(0, "", false, "", "").is_err());
        assert!(DeepLink::from_launch(-3, "", false, "", "").is_err());
    }

    // --- argv ---

    #[test]
    fn the_sober_command_is_flatpak_run_with_the_deep_link() {
        let cmd = sober_command("roblox://experiences/start?placeId=1");
        assert_eq!(cmd.program, "flatpak");
        assert_eq!(
            cmd.args,
            [
                "run",
                "--branch=stable",
                "--arch=x86_64",
                "--command=sober",
                "org.vinegarhq.Sober",
                "--",
                "roblox://experiences/start?placeId=1",
            ]
        );
    }

    #[test]
    fn the_isolated_command_wraps_the_client_in_its_own_network() {
        let link = DeepLink::from_launch(1, "", false, "", "").unwrap();
        let cmd = sober_launch_command(&link);
        assert_eq!(cmd.program, "unshare");
        assert_eq!(
            &cmd.args[..8],
            ["--user", "--map-current-user", "--net", "--", "sh", "-c", WAIT_FOR_ROUTE_SCRIPT, "sh"]
        );
        assert_eq!(&cmd.args[8..], sober_command(&link.uri()).argv().as_slice());
        // O link vai como UM argumento, sem passar por shell.
        assert_eq!(cmd.args.last().unwrap(), "roblox://experiences/start?placeId=1");
    }

    #[test]
    fn the_route_script_hands_the_command_over_untouched() {
        assert!(WAIT_FOR_ROUTE_SCRIPT.ends_with("exec \"$@\""));
        assert!(WAIT_FOR_ROUTE_SCRIPT.contains("/proc/net/route"));
        // Nunca fica preso: depois do limite o cliente abre do mesmo jeito.
        assert!(WAIT_FOR_ROUTE_SCRIPT.contains(&format!("-lt {ROUTE_WAIT_TENTHS}")));
    }

    #[test]
    fn pasta_connects_the_client_pid() {
        let cmd = pasta_command(4242);
        assert_eq!(cmd.program, "pasta");
        assert_eq!(cmd.args, ["--config-net", "4242"]);
    }

    // --- runner ---

    fn settings(preferred: &str, command: &str) -> RunnerSettings {
        RunnerSettings {
            preferred_runner: preferred.to_string(),
            custom_launch_command: command.to_string(),
            custom_process_match: String::new(),
            custom_log_dir: String::new(),
        }
    }

    #[test]
    fn the_default_runner_is_sober() {
        assert_eq!(plan_runner(&settings("sober", "")).runner, Runner::Sober);
        assert_eq!(plan_runner(&settings("", "")).runner, Runner::Sober);
        assert_eq!(plan_runner(&settings(" SOBER ", "")).runner, Runner::Sober);
        assert!(plan_runner(&settings("sober", "")).warnings.is_empty());
    }

    #[test]
    fn the_custom_runner_uses_the_configured_command() {
        let plan = plan_runner(&settings("custom", "my-roblox --profile \"a b\" {uri}"));
        let Runner::Custom(spec) = plan.runner else { panic!("expected custom") };
        assert_eq!(spec.program, "my-roblox");
        assert_eq!(spec.args, ["--profile", "a b", "{uri}"]);
    }

    #[test]
    fn the_custom_runner_without_a_command_falls_back_to_sober() {
        let plan = plan_runner(&settings("custom", "   "));
        assert_eq!(plan.runner, Runner::Sober);
        assert_eq!(plan.warnings.len(), 1);
        let broken = plan_runner(&settings("custom", "run \"oops"));
        assert_eq!(broken.runner, Runner::Sober);
        assert_eq!(broken.warnings.len(), 1);
    }

    #[test]
    fn an_unknown_runner_never_blocks_the_launch() {
        let plan = plan_runner(&settings("cordial", ""));
        assert_eq!(plan.runner, Runner::Sober);
        assert!(plan.warnings[0].render().contains("cordial"));
    }

    #[test]
    fn the_custom_command_gets_the_link_filled_in() {
        let template = split_command("run --join {uri} --who {user} --dir '{profile}'").unwrap();
        let cmd = custom_launch_command(&template, "roblox://x", Path::new("/p/1"), 9);
        assert_eq!(cmd.program, "run");
        assert_eq!(cmd.args, ["--join", "roblox://x", "--who", "9", "--dir", "/p/1"]);
    }

    #[test]
    fn the_custom_command_without_a_placeholder_gets_the_link_last() {
        let template = split_command("myrunner --fast").unwrap();
        let cmd = custom_launch_command(&template, "roblox://x", Path::new("/p"), 1);
        assert_eq!(cmd.args, ["--fast", "roblox://x"]);
    }

    #[test]
    fn the_process_match_is_parsed_and_defaults_when_empty() {
        assert_eq!(parse_process_match(" Sober, ,FlatPak "), ["sober", "flatpak"]);
        assert_eq!(
            parse_process_match(""),
            ["sober", "flatpak", "roblox", "robloxplayerbeta"]
        );
    }

    #[test]
    fn the_process_match_is_case_insensitive_and_ignores_empty_patterns() {
        let patterns = parse_process_match("sober,robloxplayerbeta");
        assert!(process_matches("/app/bin/Sober --x", &patterns));
        assert!(process_matches("RobloxPlayerBeta.exe", &patterns));
        assert!(!process_matches("firefox", &patterns));
        assert!(!process_matches("anything", &[String::new()]));
    }

    #[test]
    fn the_log_dir_comes_from_the_ini_or_from_the_profile() {
        let profile = sober_paths(Path::new("/p"));
        let mut s = settings("sober", "");
        assert_eq!(
            client_log_dir(&plan_runner(&s), &profile),
            Path::new("/p/.var/app/org.vinegarhq.Sober/data/sober/sober_logs")
        );
        s.custom_log_dir = " /var/log/x ".to_string();
        assert_eq!(client_log_dir(&plan_runner(&s), &profile), Path::new("/var/log/x"));
    }

    // --- ambiente ---

    fn host() -> HostEnv {
        HostEnv {
            home: PathBuf::from("/home/u"),
            xdg_data_home: None,
            flatpak_user_dir: None,
            xauthority: None,
            default_xauthority_exists: false,
        }
    }

    #[test]
    fn the_sober_env_points_home_at_the_profile_and_keeps_the_real_flatpak_install() {
        let plan = sober_env_plan(&host(), Path::new("/data/linux-profiles/7"), 7);
        assert_eq!(plan.get("HOME"), Some("/data/linux-profiles/7"));
        assert_eq!(
            Path::new(plan.get("FLATPAK_USER_DIR").unwrap()),
            Path::new("/home/u/.local/share/flatpak")
        );
        assert_eq!(plan.get("MULTIALT_USER_ID"), Some("7"));
        assert!(plan.remove.contains(&"XDG_DATA_HOME".to_string()));
        assert!(plan.remove.contains(&"XDG_CONFIG_HOME".to_string()));
    }

    #[test]
    fn the_x11_auth_file_follows_the_real_home() {
        let mut h = host();
        h.default_xauthority_exists = true;
        let plan = sober_env_plan(&h, Path::new("/p"), 1);
        assert_eq!(
            Path::new(plan.get("XAUTHORITY").unwrap()),
            Path::new("/home/u/.Xauthority")
        );
        // Já definido no ambiente do app: herda, não mexe.
        h.xauthority = Some(PathBuf::from("/run/user/1000/xauth"));
        assert_eq!(sober_env_plan(&h, Path::new("/p"), 1).get("XAUTHORITY"), None);
    }

    #[test]
    fn the_custom_env_keeps_the_real_home() {
        let plan = custom_env_plan(Path::new("/p/3"), 3);
        assert_eq!(plan.get("HOME"), None);
        assert_eq!(plan.get("MULTIALT_PROFILE_DIR"), Some("/p/3"));
        assert!(plan.remove.is_empty());
    }

    // --- /proc ---

    #[test]
    fn proc_stat_is_read_after_the_last_parenthesis() {
        let stat = parse_proc_stat("1234 (my (weird) name) S 99 1234 1234 0 -1 ...").unwrap();
        assert_eq!(stat, ProcStat { state: 'S', ppid: 99 });
        assert!(parse_proc_stat("garbage").is_none());
        assert!(parse_proc_stat("1 (x) S").is_none());
    }

    #[test]
    fn zombies_do_not_count_as_running() {
        assert!(is_running_state('S'));
        assert!(is_running_state('R'));
        assert!(!is_running_state('Z'));
        assert!(!is_running_state('X'));
    }

    #[test]
    fn the_process_tree_has_the_root_and_only_its_descendants() {
        // 10 -> 11 -> 12 ; 10 -> 13 ; 20 -> 21 ; 12 -> 14
        let table = [(10, 1), (11, 10), (12, 11), (13, 10), (20, 1), (21, 20), (14, 12)];
        let mut tree = process_tree(&table, 10);
        tree.sort_unstable();
        assert_eq!(tree, [10, 11, 12, 13, 14]);
        assert_eq!(process_tree(&table, 20).len(), 2);
        // Um pid que nem aparece na tabela devolve só ele mesmo.
        assert_eq!(process_tree(&table, 999), [999]);
    }

    #[test]
    fn the_process_tree_survives_a_cycle() {
        let table = [(1, 2), (2, 1)];
        let mut tree = process_tree(&table, 1);
        tree.sort_unstable();
        assert_eq!(tree, [1, 2]);
    }

    #[test]
    fn the_netns_id_is_read_from_the_link_target() {
        assert_eq!(parse_netns_id("net:[4026531840]"), Some(4026531840));
        assert_eq!(parse_netns_id("mnt:[1]"), None);
        assert_eq!(parse_netns_id("net:[abc]"), None);
    }

    // --- rede ---

    #[test]
    fn with_the_tools_the_client_gets_its_own_network() {
        assert_eq!(plan_network(true, true, 0, false), NetworkPlan::Isolated);
        assert_eq!(plan_network(true, true, 3, false), NetworkPlan::Isolated);
    }

    #[test]
    fn without_pasta_a_single_account_still_opens_with_a_warning() {
        assert_eq!(
            plan_network(true, false, 0, false),
            NetworkPlan::Plain(SoberWarning::NetworkIsolationUnavailable)
        );
        assert_eq!(
            plan_network(false, false, 0, false),
            NetworkPlan::Plain(SoberWarning::NetworkIsolationUnavailable)
        );
    }

    #[test]
    fn without_pasta_a_second_account_is_refused_with_what_to_install() {
        assert_eq!(
            plan_network(true, false, 1, false),
            NetworkPlan::Refuse(Some(SetupIssue::PastaMissing))
        );
        assert_eq!(
            plan_network(false, true, 2, false),
            NetworkPlan::Refuse(Some(SetupIssue::UnshareMissing))
        );
    }

    #[test]
    fn a_failed_isolated_attempt_falls_back_only_when_nothing_else_is_open() {
        assert_eq!(
            plan_network(true, true, 0, true),
            NetworkPlan::Plain(SoberWarning::NetworkIsolationUnavailable)
        );
        assert_eq!(plan_network(true, true, 1, true), NetworkPlan::Refuse(None));
    }

    #[test]
    fn the_plain_launch_has_no_network_wrapper() {
        let cmd = sober_command("roblox://experiences/start?placeId=1");
        assert_eq!(cmd.program, "flatpak");
        assert!(!cmd.args.contains(&"--net".to_string()));
    }

    #[test]
    fn the_network_warning_says_a_second_account_needs_passt() {
        assert!(SoberWarning::NetworkIsolationUnavailable.render().contains("passt"));
    }

    // --- diagnostico ---

    #[test]
    fn a_complete_setup_has_no_problems() {
        let d = Diagnostics {
            flatpak: Some("/usr/bin/flatpak".into()),
            sober_installed: true,
            unshare: Some("/usr/bin/unshare".into()),
            pasta: Some("/usr/bin/pasta".into()),
        };
        assert!(d.problems().is_empty());
    }

    #[test]
    fn missing_pieces_are_explained_with_the_package_to_install() {
        let none = Diagnostics::default().problems();
        assert!(none.iter().any(|m| m.contains("Flatpak")));
        assert!(none.iter().any(|m| m.contains("passt")));
        assert!(none.iter().any(|m| m.contains("util-linux")));

        let no_sober = Diagnostics {
            flatpak: Some("/usr/bin/flatpak".into()),
            sober_installed: false,
            unshare: Some("/usr/bin/unshare".into()),
            pasta: Some("/usr/bin/pasta".into()),
        };
        let problems = no_sober.problems();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("flatpak install flathub org.vinegarhq.Sober"));
    }
}
