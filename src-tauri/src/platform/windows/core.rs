struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}

/// Release signal for the dedicated thread that owns `ROBLOX_singletonMutex`.
/// Win32 mutex ownership is per-thread: ReleaseMutex only works on the thread
/// that acquired it, and the mutex is abandoned (and can be taken by a Roblox
/// client, re-enabling single-instance mode) if that thread exits. Tokio worker
/// and blocking threads give neither guarantee, so one long-lived thread
/// acquires, holds and releases it.
static MULTI_ROBLOX_HANDLE: Mutex<Option<std::sync::mpsc::Sender<()>>> = Mutex::new(None);
static COOKIES_LOCK_HANDLE: Mutex<Option<SendHandle>> = Mutex::new(None);
static TRACKER: LazyLock<ProcessTracker> = LazyLock::new(ProcessTracker::new);

fn encode_wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn tracker() -> &'static ProcessTracker {
    &TRACKER
}

pub fn generate_browser_tracker_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let a = (now % 75000 + 100000) as u64;
    let b = ((now / 31) % 800000 + 100000) as u64;
    format!("{}{}", a, b)
}

/// Sobe a thread dedicada e tenta tomar posse de `ROBLOX_singletonMutex`.
/// `Ok(None)` = o mutex está com outro processo (cliente aberto, RAM legado,
/// outra ferramenta). A thread só sobrevive se a posse for conquistada.
fn spawn_singleton_mutex_owner() -> Result<Option<std::sync::mpsc::Sender<()>>, String> {
    let (acquired_tx, acquired_rx) = std::sync::mpsc::channel::<Result<bool, String>>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("multi-roblox-mutex".into())
        .spawn(move || {
            let name = encode_wide("ROBLOX_singletonMutex");
            let h = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            if h.is_null() {
                let _ = acquired_tx.send(Err("Failed to create mutex".into()));
                return;
            }
            let result = unsafe { WaitForSingleObject(h, 0) };
            if result != WAIT_OBJECT_0 && result != WAIT_ABANDONED_0 {
                unsafe { CloseHandle(h) };
                let _ = acquired_tx.send(Ok(false));
                return;
            }
            let _ = acquired_tx.send(Ok(true));
            // Hold ownership until asked to release (or the sender is dropped).
            let _ = release_rx.recv();
            unsafe {
                ReleaseMutex(h);
                CloseHandle(h);
            }
        })
        .map_err(|e| format!("Failed to spawn mutex thread: {}", e))?;

    match acquired_rx.recv() {
        Ok(Ok(true)) => Ok(Some(release_tx)),
        Ok(Ok(false)) => Ok(None),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Mutex thread exited unexpectedly".into()),
    }
}

/// Garante que a thread dedicada esteja segurando `ROBLOX_singletonMutex`.
/// `Ok(true)` = já segurávamos ou acabamos de conquistar.
fn acquire_multi_roblox_mutex() -> Result<bool, String> {
    let mut handle = MULTI_ROBLOX_HANDLE.lock().map_err(|e| e.to_string())?;
    if handle.is_some() {
        return Ok(true);
    }
    match spawn_singleton_mutex_owner()? {
        Some(release_tx) => {
            *handle = Some(release_tx);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Habilita vários clientes Roblox simultâneos.
///
/// São duas travas diferentes, e elas **não** se substituem:
///
/// 1. `ROBLOX_singletonMutex` — trava legada. Só funciona preventivamente: se o
///    app pegar o mutex **antes** de qualquer cliente, os clientes seguintes
///    sobem sem instalar o modo instância única. Se um cliente já estiver
///    aberto (o usuário entrou pelo site), o mutex é dele e não há como tomá-lo
///    sem matar o processo — era exatamente aí que o app dizia "A Roblox client
///    is already running" ou matava os clientes do usuário.
/// 2. `ROBLOX_singletonEvent` — é o que o cliente **moderno** consulta ao subir:
///    se o nome existe, ele sinaliza a instância antiga e sai. Fechar esse
///    handle de fora (`singleton.rs`) apaga o nome sem tocar no processo: o
///    cliente aberto continua jogando e o próximo sobe normal. Verificado na
///    máquina do usuário, sem elevação.
///
/// Conclusão: para o cliente atual o **Event é o que importa**; o mutex fica
/// porque continua sendo a trava barata e preventiva (e builds antigas ainda
/// dependem dele). Por isso o mutex é mantido como estava e o fechamento do
/// Event entrou como etapa extra — inclusive no caminho de sucesso, para cobrir
/// o caso "app já segurava o mutex e o usuário abriu o jogo pelo site depois".
/// Sem cliente Roblox aberto a etapa nova não custa nada (sai na checagem de
/// pids) e o comportamento antigo é idêntico.
pub fn enable_multi_roblox() -> Result<bool, String> {
    if acquire_multi_roblox_mutex()? {
        // Um cliente aberto fora do app pode ter publicado o Event mesmo com o
        // mutex na nossa mão; limpar é barato e não fecha ninguém.
        let _ = close_roblox_singleton_handles();
        lock_roblox_cookies()?;
        return Ok(true);
    }

    // Mutex ocupado. Antes de desistir (e antes que o chamador caia no último
    // recurso de matar clientes com `AutoCloseRobloxForMultiRbx`), destrava
    // pelo Event.
    if close_roblox_singleton_handles() == 0 {
        // Nada de Roblox para destravar: quem segura o mutex é outra coisa
        // (RAM legado, outra ferramenta). Caminho antigo.
        return Ok(false);
    }

    // O mutex pode ter sido liberado nesse meio tempo; se não foi, seguimos
    // assim mesmo — sem o Event o cliente novo não desiste mais.
    let _ = acquire_multi_roblox_mutex()?;
    lock_roblox_cookies()?;
    Ok(true)
}

fn release_multi_roblox_mutex() {
    if let Ok(mut handle) = MULTI_ROBLOX_HANDLE.lock() {
        if let Some(release) = handle.take() {
            let _ = release.send(());
        }
    }
}

pub fn release_multi_roblox_handle() {
    release_multi_roblox_mutex();
    if let Ok(mut cookie_handle) = COOKIES_LOCK_HANDLE.lock() {
        if let Some(SendHandle(h)) = cookie_handle.take() {
            unsafe {
                CloseHandle(h);
            }
        }
    }
}

pub fn this_process_holds_multi_roblox() -> bool {
    MULTI_ROBLOX_HANDLE
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

pub fn disable_multi_roblox() -> Result<(), String> {
    release_multi_roblox_mutex();

    lock_roblox_cookies()?;
    Ok(())
}

fn lock_roblox_cookies() -> Result<(), String> {
    let mut handle = COOKIES_LOCK_HANDLE.lock().map_err(|e| e.to_string())?;
    if handle.is_some() || is_773_fix_disabled() {
        return Ok(());
    }

    let Some(path) = get_roblox_cookies_path() else {
        return Ok(());
    };

    let wide = encode_wide(path.as_os_str());
    unsafe {
        let cookie_handle = CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if cookie_handle == INVALID_HANDLE_VALUE {
            eprintln!("Warning: Could not lock RobloxCookies.dat for the 773 fix");
            return Ok(());
        }
        *handle = Some(SendHandle(cookie_handle));
    }

    Ok(())
}

fn get_roblox_cookies_path() -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    let path = PathBuf::from(local_app_data)
        .join("Roblox")
        .join("LocalStorage")
        .join("RobloxCookies.dat");
    path.exists().then_some(path)
}

fn is_773_fix_disabled() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join("no773fix.txt")))
        .map(|path| path.exists())
        .unwrap_or(false)
}

pub fn get_roblox_path() -> Result<String, String> {
    // The registry handler can point at an account test-channel build (see
    // `launch_url`); prefer the production build once it has been resolved.
    if let Some(dir) = cached_production_player_dir() {
        return Ok(dir);
    }

    unsafe {
        let key_name = encode_wide("roblox\\DefaultIcon");
        let mut hkey: windows_sys::Win32::System::Registry::HKEY = std::ptr::null_mut();

        if RegOpenKeyExW(HKEY_CLASSES_ROOT, key_name.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
            let mut buf = [0u16; 512];
            let mut buf_size = (buf.len() * 2) as u32;
            let mut value_type = 0u32;

            let result = RegQueryValueExW(
                hkey,
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut value_type,
                buf.as_mut_ptr() as *mut u8,
                &mut buf_size,
            );

            RegCloseKey(hkey);

            if result == 0 && value_type == REG_SZ {
                let len = (buf_size as usize / 2).saturating_sub(1);
                let path = String::from_utf16_lossy(&buf[..len]);
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    if parent.exists() {
                        return Ok(parent.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }

    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let versions_dir = format!("{}\\Roblox\\Versions", local_app_data);

    if let Ok(entries) = std::fs::read_dir(&versions_dir) {
        let mut best: Option<(SystemTime, String)> = None;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("version-") && entry.path().join("RobloxPlayerBeta.exe").exists() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if best.as_ref().map_or(true, |(t, _)| modified > *t) {
                            best = Some((modified, entry.path().to_string_lossy().into_owned()));
                        }
                    }
                }
            }
        }
        if let Some((_, path)) = best {
            return Ok(path);
        }
    }

    Err("Roblox installation not found".into())
}

fn get_client_settings_file() -> Result<PathBuf, String> {
    let version_folder = get_roblox_path()?;
    let settings_dir = std::path::Path::new(&version_folder).join("ClientSettings");

    if !settings_dir.exists() {
        std::fs::create_dir_all(&settings_dir)
            .map_err(|e| format!("Failed to create ClientSettings: {}", e))?;
    }

    Ok(settings_dir.join("ClientAppSettings.json"))
}

#[cfg(test)]
mod browser_tracker_tests {
    use super::*;

    #[test]
    fn generate_browser_tracker_id_is_digits_only_and_of_the_expected_length() {
        for _ in 0..50 {
            let id = generate_browser_tracker_id();
            assert!(
                id.chars().all(|c| c.is_ascii_digit()),
                "expected digits only, got {id}"
            );
            assert!(
                (11..=13).contains(&id.len()),
                "unexpected length {} for {id}",
                id.len()
            );
            // Both halves are built with a +100000 floor, so neither can be
            // shorter than six digits nor start with a zero.
            assert!(!id.starts_with('0'), "unexpected leading zero in {id}");
        }
    }

    #[test]
    fn generate_browser_tracker_id_varies_between_calls() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..25 {
            seen.insert(generate_browser_tracker_id());
        }
        assert!(seen.len() > 1, "tracker ids should not be constant");
    }
}
