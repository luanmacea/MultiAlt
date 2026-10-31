// O que há instalado: flatpak, Sober, unshare e pasta.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::platform::sober::{self, Diagnostics, HostEnv};

/// Procura `name` executável em cada pasta do `PATH`.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path: OsString = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// O ambiente do usuário como o app o enxerga (HOME real, não o do perfil).
pub fn host_env() -> HostEnv {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let non_empty =
        |key: &str| std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    HostEnv {
        default_xauthority_exists: home.join(".Xauthority").exists(),
        xdg_data_home: non_empty("XDG_DATA_HOME"),
        flatpak_user_dir: non_empty("FLATPAK_USER_DIR"),
        xauthority: non_empty("XAUTHORITY"),
        home,
    }
}

pub fn diagnose() -> Diagnostics {
    let host = host_env();
    let user_dir = sober::flatpak_user_dir(
        host.flatpak_user_dir.as_deref(),
        host.xdg_data_home.as_deref(),
        &host.home,
    );
    Diagnostics {
        flatpak: find_in_path("flatpak"),
        sober_installed: sober::sober_install_candidates(&user_dir)
            .iter()
            .any(|p| p.is_dir()),
        unshare: find_in_path("unshare"),
        pasta: find_in_path("pasta"),
    }
}
