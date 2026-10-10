// Volume ao vivo por cliente (ideia 20 de docs/ideias-de-outros-gerenciadores.md).
//
// Com `Optimization.MuteBackgroundClients` ligado, os clientes que o app abriu
// ficam mudos, menos o cliente em uso (o mesmo de `focus_follow.rs`). É o
// mixer de volume do Windows (sessões de áudio por processo), com o jogo
// aberto — não mexe no arquivo de configurações do Roblox.
//
// **Atrás da feature `live-audio` do Cargo, nas duas edições** (entra pelo
// `standard`, que o `full` inclui; decisão do dono, 10/10/2026). A sessão de
// áudio só se alcança por COM (`IAudioSessionManager2`), código nativo novo no
// binário, por isso fica isolada atrás da feature: tirá-la de uma edição é
// tirá-la da lista. Sem a feature este arquivo compila só a decisão (função
// pura, testada) e a opção some da tela (`supportsLiveAudio = false`). Ver
// docs/features/performance.md.
//
// Sem crate novo: o `windows-sys` não traz interfaces COM, então as cinco
// tabelas de métodos usadas (enumerador de dispositivos, dispositivo, gerenciador
// de sessões, sessão, volume simples) estão escritas à mão em `live_audio_com`,
// só com os métodos chamados; o resto é preenchido com `usize` para manter a
// ordem. A feature acrescenta ao `windows-sys` apenas `Win32_System_Com`
// (`CoInitializeEx`, `CoCreateInstance`, `CoUninitialize`).
//
// Regras:
// - **Só clientes que o app abriu.** Cliente aberto pelo site nunca é mutado.
// - Sem cliente em uso (nenhum cliente nosso esteve em primeiro plano ainda),
//   ninguém é mutado.
// - O app só desmuta o que ele mesmo mutou: um mudo que o usuário pôs no mixer
//   fica.
// - Desligar a opção ou fechar o app desmuta tudo que o app mutou.

/// A feature `live-audio` entrou neste binário?
pub(crate) const LIVE_AUDIO_COMPILED: bool = cfg!(feature = "live-audio");

/// Intervalo entre tentativas para um cliente que ainda não tem sessão de
/// áudio (o Roblox só abre a sessão quando o jogo começa a tocar).
const LIVE_AUDIO_RETRY: Duration = Duration::from_secs(5);

/// O cliente deve estar mudo? Só se existe cliente em uso e não é ele.
fn desired_mute(pid: u32, active: Option<u32>) -> bool {
    active.is_some_and(|active| active != pid)
}

/// O que mudar: `(pid, mutar?)` para cada cliente nosso cujo estado desejado
/// difere do que o app fez. Ordenado por PID.
fn plan_mute_changes(
    ours: &std::collections::HashSet<u32>,
    active: Option<u32>,
    muted_by_us: &std::collections::HashSet<u32>,
) -> Vec<(u32, bool)> {
    let mut out: Vec<(u32, bool)> = ours
        .iter()
        .filter_map(|&pid| {
            let want = desired_mute(pid, active);
            (want != muted_by_us.contains(&pid)).then_some((pid, want))
        })
        .collect();
    out.sort_by_key(|(pid, _)| *pid);
    out
}

/// Ao desligar: desmutar exatamente o que o app mutou.
fn plan_unmute_all(muted_by_us: &std::collections::HashSet<u32>) -> Vec<(u32, bool)> {
    let mut out: Vec<(u32, bool)> = muted_by_us.iter().map(|&pid| (pid, false)).collect();
    out.sort_by_key(|(pid, _)| *pid);
    out
}

/// `Optimization.MuteBackgroundClients` (desligado por padrão). Sem a feature
/// no binário é sempre falso.
pub(crate) fn mute_background_enabled(settings: &SettingsStore) -> bool {
    LIVE_AUDIO_COMPILED && settings.get_bool("Optimization", "MuteBackgroundClients")
}

/// Aplica as mudanças e devolve, por PID, quantas sessões de áudio mudaram.
/// 0 = o cliente ainda não tem sessão (tentar de novo depois).
#[cfg(feature = "live-audio")]
fn set_mute_for_pids(changes: &[(u32, bool)]) -> Result<HashMap<u32, usize>, String> {
    live_audio_com::set_mute_for_pids(changes)
}

#[cfg(not(feature = "live-audio"))]
fn set_mute_for_pids(_changes: &[(u32, bool)]) -> Result<HashMap<u32, usize>, String> {
    Err("live-audio is not part of this build".into())
}

#[derive(Default)]
struct LiveAudioState {
    muted_by_us: std::collections::HashSet<u32>,
    last_active: Option<u32>,
    last_ours: Vec<u32>,
    next_retry: Option<Instant>,
}

static LIVE_AUDIO: LazyLock<Mutex<LiveAudioState>> =
    LazyLock::new(|| Mutex::new(LiveAudioState::default()));

fn live_audio_state() -> std::sync::MutexGuard<'static, LiveAudioState> {
    LIVE_AUDIO.lock().unwrap_or_else(|e| e.into_inner())
}

/// Executa um plano e anota o resultado. Volta `true` se sobrou mudança sem
/// sessão para aplicar (o cliente ainda não toca nada).
fn run_mute_plan(state: &mut LiveAudioState, plan: &[(u32, bool)]) -> bool {
    // O Windows reaproveita PIDs: só mexe no áudio de quem ainda é Roblox.
    let alive: std::collections::HashSet<u32> = get_roblox_pids().into_iter().collect();
    let (plan, gone): (Vec<(u32, bool)>, Vec<(u32, bool)>) =
        plan.iter().partition(|(pid, _)| alive.contains(pid));
    for (pid, _) in gone {
        state.muted_by_us.remove(&pid);
    }
    if plan.is_empty() {
        return false;
    }
    let plan = plan.as_slice();
    let changed = match set_mute_for_pids(plan) {
        Ok(changed) => changed,
        Err(err) => {
            eprintln!("[audio] {err}");
            return true;
        }
    };
    let mut pending = false;
    for &(pid, mute) in plan {
        let done = changed.get(&pid).copied().unwrap_or(0) > 0;
        if mute {
            if done {
                state.muted_by_us.insert(pid);
            } else {
                pending = true;
            }
        } else {
            // Desmutar sem sessão = a sessão sumiu junto com o mudo: está feito.
            state.muted_by_us.remove(&pid);
        }
    }
    pending
}

/// A parte de áudio da olhada de cada segundo (`focus_follow_tick`).
fn live_audio_tick(
    enabled: bool,
    ours: &std::collections::HashSet<u32>,
    active: Option<u32>,
) {
    let mut state = live_audio_state();
    state.muted_by_us.retain(|pid| ours.contains(pid));

    if !enabled {
        if !state.muted_by_us.is_empty() {
            let plan = plan_unmute_all(&state.muted_by_us);
            run_mute_plan(&mut state, &plan);
        }
        state.last_active = None;
        state.last_ours.clear();
        state.next_retry = None;
        return;
    }

    let plan = plan_mute_changes(ours, active, &state.muted_by_us);
    let mut ours_sorted: Vec<u32> = ours.iter().copied().collect();
    ours_sorted.sort_unstable();
    let now = Instant::now();
    let something_moved = state.last_active != active || state.last_ours != ours_sorted;
    let retry_due = state.next_retry.is_some_and(|at| now >= at);
    state.last_active = active;
    state.last_ours = ours_sorted;
    if plan.is_empty() {
        state.next_retry = None;
        return;
    }
    if !something_moved && !retry_due {
        return;
    }
    let pending = run_mute_plan(&mut state, &plan);
    state.next_retry = pending.then(|| now + LIVE_AUDIO_RETRY);
}

/// Ao fechar o app: desmuta o que o app mutou.
pub fn release_live_audio() {
    let mut state = live_audio_state();
    if !state.muted_by_us.is_empty() {
        let plan = plan_unmute_all(&state.muted_by_us);
        run_mute_plan(&mut state, &plan);
    }
}

#[cfg(feature = "live-audio")]
mod live_audio_com {
    //! COM do mixer de volume, à mão (ver o topo do arquivo). Tabelas só até o
    //! último método usado; a ordem é a do SDK (`mmdeviceapi.h`,
    //! `audiopolicy.h`).

    use std::collections::HashMap;
    use std::ffi::c_void;
    use windows_sys::core::{GUID, HRESULT};
    use windows_sys::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    const CLSID_MM_DEVICE_ENUMERATOR: GUID =
        GUID::from_u128(0xbcde0395_e52f_467c_8e3d_c4579291692e);
    const IID_IMM_DEVICE_ENUMERATOR: GUID =
        GUID::from_u128(0xa95664d2_9614_4f35_a746_de8db63617e6);
    const IID_IAUDIO_SESSION_MANAGER2: GUID =
        GUID::from_u128(0x77aa99a0_1bd6_484f_8bc7_2c654c9a9b6f);
    const IID_IAUDIO_SESSION_CONTROL2: GUID =
        GUID::from_u128(0xbfb7ff88_7239_4fc9_8fa2_07c950be9c6d);
    const IID_ISIMPLE_AUDIO_VOLUME: GUID =
        GUID::from_u128(0x87ce5498_68d6_44e5_9215_6da47ef883d8);

    /// `eRender`: dispositivos de saída.
    const E_RENDER: i32 = 0;
    const DEVICE_STATE_ACTIVE: u32 = 1;
    /// A thread já está num apartamento COM de outro tipo: dá para usar, mas
    /// não é nossa para desinicializar.
    const RPC_E_CHANGED_MODE: HRESULT = 0x8001_0106_u32 as i32;

    type Ptr = *mut c_void;
    type Unused = usize;

    #[repr(C)]
    struct IUnknownVtbl {
        query_interface: unsafe extern "system" fn(Ptr, *const GUID, *mut Ptr) -> HRESULT,
        add_ref: unsafe extern "system" fn(Ptr) -> u32,
        release: unsafe extern "system" fn(Ptr) -> u32,
    }

    #[repr(C)]
    struct IMMDeviceEnumeratorVtbl {
        base: IUnknownVtbl,
        enum_audio_endpoints: unsafe extern "system" fn(Ptr, i32, u32, *mut Ptr) -> HRESULT,
    }

    #[repr(C)]
    struct IMMDeviceCollectionVtbl {
        base: IUnknownVtbl,
        get_count: unsafe extern "system" fn(Ptr, *mut u32) -> HRESULT,
        item: unsafe extern "system" fn(Ptr, u32, *mut Ptr) -> HRESULT,
    }

    #[repr(C)]
    struct IMMDeviceVtbl {
        base: IUnknownVtbl,
        activate: unsafe extern "system" fn(Ptr, *const GUID, u32, *const c_void, *mut Ptr) -> HRESULT,
    }

    #[repr(C)]
    struct IAudioSessionManager2Vtbl {
        base: IUnknownVtbl,
        // IAudioSessionManager
        _get_audio_session_control: Unused,
        _get_simple_audio_volume: Unused,
        // IAudioSessionManager2
        get_session_enumerator: unsafe extern "system" fn(Ptr, *mut Ptr) -> HRESULT,
    }

    #[repr(C)]
    struct IAudioSessionEnumeratorVtbl {
        base: IUnknownVtbl,
        get_count: unsafe extern "system" fn(Ptr, *mut i32) -> HRESULT,
        get_session: unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> HRESULT,
    }

    #[repr(C)]
    struct IAudioSessionControl2Vtbl {
        base: IUnknownVtbl,
        // IAudioSessionControl: GetState, GetDisplayName, SetDisplayName,
        // GetIconPath, SetIconPath, GetGroupingParam, SetGroupingParam,
        // RegisterAudioSessionNotification, UnregisterAudioSessionNotification
        _control: [Unused; 9],
        // IAudioSessionControl2: GetSessionIdentifier, GetSessionInstanceIdentifier
        _identifiers: [Unused; 2],
        get_process_id: unsafe extern "system" fn(Ptr, *mut u32) -> HRESULT,
    }

    #[repr(C)]
    struct ISimpleAudioVolumeVtbl {
        base: IUnknownVtbl,
        _set_master_volume: Unused,
        _get_master_volume: Unused,
        set_mute: unsafe extern "system" fn(Ptr, i32, *const GUID) -> HRESULT,
    }

    /// Ponteiro COM que solta a referência ao sair de escopo.
    struct Com(Ptr);

    impl Com {
        /// # Safety
        /// `T` tem que ser a tabela da interface que este ponteiro é.
        unsafe fn vtbl<T>(&self) -> &T {
            &**(self.0 as *mut *const T)
        }

        fn query(&self, iid: &GUID) -> Option<Com> {
            let mut out: Ptr = std::ptr::null_mut();
            let hr = unsafe { (self.vtbl::<IUnknownVtbl>().query_interface)(self.0, iid, &mut out) };
            (hr >= 0 && !out.is_null()).then_some(Com(out))
        }
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    (self.vtbl::<IUnknownVtbl>().release)(self.0);
                }
            }
        }
    }

    fn out_ptr(hr: HRESULT, ptr: Ptr) -> Option<Com> {
        (hr >= 0 && !ptr.is_null()).then_some(Com(ptr))
    }

    /// Apartamento COM da thread enquanto o valor viver.
    struct Apartment {
        owned: bool,
    }

    impl Apartment {
        fn enter() -> Result<Self, String> {
            let hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
            if hr >= 0 {
                Ok(Self { owned: true })
            } else if hr == RPC_E_CHANGED_MODE {
                Ok(Self { owned: false })
            } else {
                Err(format!("CoInitializeEx failed (0x{:08x})", hr as u32))
            }
        }
    }

    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.owned {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// As sessões de áudio de todos os dispositivos de saída ativos.
    fn audio_sessions() -> Result<Vec<Com>, String> {
        let mut raw: Ptr = std::ptr::null_mut();
        let hr = unsafe {
            CoCreateInstance(
                &CLSID_MM_DEVICE_ENUMERATOR,
                std::ptr::null_mut(),
                CLSCTX_ALL,
                &IID_IMM_DEVICE_ENUMERATOR,
                &mut raw,
            )
        };
        let enumerator =
            out_ptr(hr, raw).ok_or_else(|| format!("No audio device enumerator (0x{:08x})", hr as u32))?;

        let mut raw: Ptr = std::ptr::null_mut();
        let hr = unsafe {
            (enumerator.vtbl::<IMMDeviceEnumeratorVtbl>().enum_audio_endpoints)(
                enumerator.0,
                E_RENDER,
                DEVICE_STATE_ACTIVE,
                &mut raw,
            )
        };
        let devices = out_ptr(hr, raw).ok_or_else(|| "Could not list audio devices".to_string())?;
        let mut device_count = 0u32;
        unsafe { (devices.vtbl::<IMMDeviceCollectionVtbl>().get_count)(devices.0, &mut device_count) };

        let mut sessions = Vec::new();
        for i in 0..device_count {
            let mut raw: Ptr = std::ptr::null_mut();
            let hr = unsafe { (devices.vtbl::<IMMDeviceCollectionVtbl>().item)(devices.0, i, &mut raw) };
            let Some(device) = out_ptr(hr, raw) else { continue };

            let mut raw: Ptr = std::ptr::null_mut();
            let hr = unsafe {
                (device.vtbl::<IMMDeviceVtbl>().activate)(
                    device.0,
                    &IID_IAUDIO_SESSION_MANAGER2,
                    CLSCTX_ALL,
                    std::ptr::null(),
                    &mut raw,
                )
            };
            let Some(manager) = out_ptr(hr, raw) else { continue };

            let mut raw: Ptr = std::ptr::null_mut();
            let hr = unsafe {
                (manager.vtbl::<IAudioSessionManager2Vtbl>().get_session_enumerator)(manager.0, &mut raw)
            };
            let Some(list) = out_ptr(hr, raw) else { continue };
            let mut count = 0i32;
            unsafe { (list.vtbl::<IAudioSessionEnumeratorVtbl>().get_count)(list.0, &mut count) };
            for j in 0..count.max(0) {
                let mut raw: Ptr = std::ptr::null_mut();
                let hr = unsafe {
                    (list.vtbl::<IAudioSessionEnumeratorVtbl>().get_session)(list.0, j, &mut raw)
                };
                if let Some(session) = out_ptr(hr, raw) {
                    sessions.push(session);
                }
            }
        }
        Ok(sessions)
    }

    /// O PID dono da sessão — só se for de um processo só (`S_OK`); sessão
    /// compartilhada (`AUDCLNT_S_NO_SINGLE_PROCESS`) não é de cliente nenhum.
    fn session_pid(session: &Com) -> Option<u32> {
        let control = session.query(&IID_IAUDIO_SESSION_CONTROL2)?;
        let mut pid = 0u32;
        let hr = unsafe {
            (control.vtbl::<IAudioSessionControl2Vtbl>().get_process_id)(control.0, &mut pid)
        };
        (hr == 0 && pid != 0).then_some(pid)
    }

    pub(super) fn set_mute_for_pids(changes: &[(u32, bool)]) -> Result<HashMap<u32, usize>, String> {
        let wanted: HashMap<u32, bool> = changes.iter().copied().collect();
        let _apartment = Apartment::enter()?;
        let mut changed: HashMap<u32, usize> = HashMap::new();
        for session in audio_sessions()? {
            let Some(pid) = session_pid(&session) else { continue };
            let Some(&mute) = wanted.get(&pid) else { continue };
            let Some(volume) = session.query(&IID_ISIMPLE_AUDIO_VOLUME) else { continue };
            let hr = unsafe {
                (volume.vtbl::<ISimpleAudioVolumeVtbl>().set_mute)(
                    volume.0,
                    i32::from(mute),
                    std::ptr::null(),
                )
            };
            if hr >= 0 {
                *changed.entry(pid).or_insert(0) += 1;
            }
        }
        Ok(changed)
    }

    #[cfg(test)]
    mod live_audio_com_tests {
        use super::*;

        #[test]
        fn an_empty_plan_only_reads_the_mixer_and_changes_nothing() {
            // Percorre as tabelas escritas à mão (enumerador, dispositivos,
            // sessões, PID de cada sessão) sem chamar SetMute: uma tabela fora
            // de ordem derruba o teste. Máquina sem áudio (CI) pode dar Err.
            match set_mute_for_pids(&[]) {
                Ok(changed) => assert!(changed.is_empty()),
                Err(err) => eprintln!("sem mixer nesta máquina: {err}"),
            }
        }

        #[test]
        fn the_tables_stop_at_the_last_method_used() {
            let word = std::mem::size_of::<usize>();
            assert_eq!(std::mem::size_of::<IAudioSessionManager2Vtbl>(), 6 * word);
            assert_eq!(std::mem::size_of::<IAudioSessionControl2Vtbl>(), 15 * word);
            assert_eq!(std::mem::size_of::<ISimpleAudioVolumeVtbl>(), 6 * word);
            assert_eq!(std::mem::size_of::<IMMDeviceVtbl>(), 4 * word);
        }
    }
}

#[cfg(test)]
mod live_audio_tests {
    use super::*;
    use std::collections::HashSet;

    fn set(pids: &[u32]) -> HashSet<u32> {
        pids.iter().copied().collect()
    }

    #[test]
    fn only_the_clients_not_in_use_are_muted() {
        assert!(!desired_mute(10, Some(10)));
        assert!(desired_mute(20, Some(10)));
    }

    #[test]
    fn with_no_client_in_use_nobody_is_muted() {
        assert!(!desired_mute(10, None));
        assert!(plan_mute_changes(&set(&[10, 20]), None, &set(&[])).is_empty());
    }

    #[test]
    fn switching_windows_mutes_the_old_one_and_unmutes_the_new_one() {
        assert_eq!(
            plan_mute_changes(&set(&[10, 20, 30]), Some(20), &set(&[20, 30])),
            vec![(10, true), (20, false)]
        );
    }

    #[test]
    fn nothing_changes_when_the_state_is_already_right() {
        assert!(plan_mute_changes(&set(&[10, 20]), Some(10), &set(&[20])).is_empty());
    }

    #[test]
    fn a_client_opened_from_the_website_is_never_muted() {
        // O adotado não está em `ours`: nenhum plano o cita.
        let plan = plan_mute_changes(&set(&[10, 20]), Some(10), &set(&[]));
        assert_eq!(plan, vec![(20, true)]);
        assert!(plan.iter().all(|(pid, _)| *pid != 77));
    }

    #[test]
    fn a_mute_the_user_set_is_never_undone() {
        // O cliente em uso (10) não foi mutado pelo app: o plano não o
        // desmuta, mesmo que o usuário o tenha mutado no mixer.
        let plan = plan_mute_changes(&set(&[10]), Some(10), &set(&[]));
        assert!(plan.is_empty());
    }

    #[test]
    fn turning_it_off_unmutes_exactly_what_the_app_muted() {
        assert_eq!(plan_unmute_all(&set(&[30, 20])), vec![(20, false), (30, false)]);
        assert!(plan_unmute_all(&set(&[])).is_empty());
    }

    #[test]
    fn the_option_is_off_by_default_and_needs_the_build_feature() {
        let path = std::env::temp_dir().join(format!(
            "ram4-live-audio-{}-{}.ini",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let settings = SettingsStore::new(path.clone());
        assert!(!mute_background_enabled(&settings));
        settings
            .set("Optimization", "MuteBackgroundClients", "true")
            .unwrap();
        assert_eq!(mute_background_enabled(&settings), LIVE_AUDIO_COMPILED);
        let _ = std::fs::remove_file(&path);
    }

    /// O que decide em que edição a feature vai: o Cargo.toml (lista do
    /// `standard`, que o `full` inclui) e as linhas de build do workflow.
    /// Decisão do dono (10/10/2026): o volume ao vivo vai nas duas edições.
    #[test]
    fn live_audio_ships_in_both_editions() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        let feature_line = |name: &str| {
            manifest
                .lines()
                .find(|l| l.trim_start().starts_with(&format!("{name} =")))
                .unwrap_or_else(|| panic!("feature {name}"))
                .to_string()
        };
        let standard_line = feature_line("standard");
        assert!(
            standard_line.contains("\"live-audio\""),
            "live-audio saiu da edição padrão: {standard_line}"
        );
        let full_line = feature_line("full");
        assert!(
            full_line.contains("\"standard\"") || full_line.contains("\"live-audio\""),
            "live-audio saiu da edição completa: {full_line}"
        );
        assert!(
            manifest.contains("live-audio = [\"windows-sys/Win32_System_Com\"]"),
            "a feature live-audio só pode ligar o COM do windows-sys"
        );
        // A padrão compila `--no-default-features --features standard`, a
        // completa `--features full`: nenhuma das duas lista features à parte.
        let workflow =
            std::fs::read_to_string(root.join("../.github/workflows/release-v4.yml")).unwrap();
        let standard_args: Vec<&str> = workflow
            .lines()
            .filter(|l| l.contains("--no-default-features"))
            .collect();
        assert!(!standard_args.is_empty(), "build da edição padrão sumiu do workflow");
        for line in &standard_args {
            assert!(
                line.contains("--features standard"),
                "a edição padrão não compila a feature standard: {line}"
            );
        }
        assert!(!workflow.contains("live-audio"), "o workflow não lista features à parte");
    }

    /// COM fica isolado aqui, atrás da feature: nenhum outro arquivo do
    /// backend chama estas APIs.
    #[test]
    fn com_calls_live_only_behind_the_live_audio_feature() {
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push((
                        path.to_string_lossy().replace('\\', "/"),
                        std::fs::read_to_string(&path).unwrap(),
                    ));
                }
            }
        }
        let mut files = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        for (path, body) in &files {
            if path.ends_with("platform/windows/live_audio.rs") {
                continue;
            }
            for api in ["CoCreateInstance", "CoInitializeEx", "Win32::System::Com"] {
                assert!(!body.contains(api), "{path} usa {api}");
            }
        }
        let this = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/platform/windows/live_audio.rs"),
        )
        .unwrap();
        let com_at = this.find("mod live_audio_com").unwrap();
        assert!(this[..com_at].trim_end().ends_with("#[cfg(feature = \"live-audio\")]"));
    }
}
