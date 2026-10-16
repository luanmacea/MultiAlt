// Destrava o modo "instância única" do Roblox **sem fechar** os clientes já
// abertos.
//
// O cliente moderno publica um evento nomeado
// `\Sessions\<n>\BaseNamedObjects\ROBLOX_singletonEvent`. Quando um novo
// cliente sobe e esse nome existe, ele sinaliza a instância antiga e sai — é
// daí que vem o "A Roblox client is already running". Fechando o *handle*
// dentro do processo do cliente aberto (exatamente o que o Process Explorer
// faz, com `DuplicateHandle` + `DUPLICATE_CLOSE_SOURCE`) o nome some do
// namespace, o cliente aberto continua rodando normalmente e a próxima
// instância sobe sem reclamar. Validado na máquina do usuário como usuário
// comum: `OpenProcess(PROCESS_DUP_HANDLE)` no RobloxPlayerBeta funciona sem
// elevação.
//
// Para achar o handle é preciso varrer a tabela de handles do sistema
// (`NtQuerySystemInformation(SystemExtendedHandleInformation)`), que o
// `windows-sys` não expõe — daí as declarações `extern "system"` abaixo.
//
// Cuidados que o código respeita (a varredura mexe em processo de terceiros):
// - só processos cujo executável é do Roblox (`find_roblox_pids_all`);
// - só handles do **tipo Event** — o índice do tipo é descoberto criando um
//   evento nosso e localizando-o na tabela, o que evita chamar `NtQueryObject`
//   num handle de pipe (pode travar a chamada indefinidamente);
// - o nome tem que terminar **exatamente** em `ROBLOX_singletonEvent`, na
//   fronteira do namespace (`\`), comparação sem diferenciar maiúsculas;
// - nenhuma falha de API vira pânico: tudo devolve 0 / `None`.

// Importados aqui (e não no topo de `windows.rs`) porque só este arquivo usa.
use windows_sys::Win32::Foundation::{
    DuplicateHandle, GetLastError, DUPLICATE_CLOSE_SOURCE, DUPLICATE_SAME_ACCESS,
    ERROR_FILE_NOT_FOUND, ERROR_INVALID_HANDLE,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, OpenEventW, EVENT_MODIFY_STATE, PROCESS_DUP_HANDLE,
};

/// Sufixo do evento de instância única do cliente Roblox.
const ROBLOX_SINGLETON_EVENT: &str = "ROBLOX_singletonEvent";

/// `SystemExtendedHandleInformation` (classe de `NtQuerySystemInformation`).
const SYSTEM_EXTENDED_HANDLE_INFORMATION: u32 = 0x40;
/// `ObjectNameInformation` (classe de `NtQueryObject`).
const OBJECT_NAME_INFORMATION: u32 = 1;
/// `STATUS_INFO_LENGTH_MISMATCH` — buffer curto, tentar de novo maior.
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004u32 as i32;
/// Teto do buffer da tabela de handles; acima disso desistimos em vez de
/// arriscar uma alocação absurda. Era 64 MB até 03/10/2026, quando um programa
/// de periférico vazando handles (NGenuity2Helper, 1,77 milhão) levou a tabela de
/// uma máquina real a ~77 MB e o singleton do Roblox deixou de ser fechado. A
/// alocação é passageira (só durante a leitura), então 256 MB é folga, não custo.
const HANDLE_SNAPSHOT_MAX_BYTES: usize = 256 * 1024 * 1024;
/// Buffer de 4 KiB (em `u64`, para garantir alinhamento) para o nome do objeto.
const OBJECT_NAME_BUFFER_WORDS: usize = 512;

#[allow(non_snake_case)]
#[link(name = "ntdll")]
extern "system" {
    fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut std::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;

    fn NtQueryObject(
        handle: HANDLE,
        object_information_class: u32,
        object_information: *mut std::ffi::c_void,
        object_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

/// `SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX`. Os campos não lidos ficam declarados
/// porque o layout precisa bater byte a byte com o do kernel.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct SystemHandleEntryEx {
    object: usize,
    unique_process_id: usize,
    handle_value: usize,
    granted_access: u32,
    creator_back_trace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

/// O nome do objeto termina exatamente em `suffix`?
///
/// Comparação sem diferenciar maiúsculas e **na fronteira do namespace**: o que
/// vier antes do sufixo tem que ser `\` (ou nada). Sem essa checagem um objeto
/// chamado `OUTRACOISA_ROBLOX_singletonEvent` casaria — e o contrato aqui é
/// nunca fechar handle que não seja exatamente o alvo.
fn singleton_name_matches(name: &str, suffix: &str) -> bool {
    if suffix.is_empty() {
        return false;
    }
    let Some(split) = name.len().checked_sub(suffix.len()) else {
        return false;
    };
    let Some(tail) = name.get(split..) else {
        return false;
    };
    if !tail.eq_ignore_ascii_case(suffix) {
        return false;
    }
    match name.get(..split).and_then(|head| head.chars().next_back()) {
        None => true,
        Some('\\') => true,
        Some(_) => false,
    }
}

/// Copia a tabela de handles do sistema. Devolve vazio em qualquer falha.
/// Próximo tamanho do buffer depois de um `STATUS_INFO_LENGTH_MISMATCH`.
///
/// Cresce para o que o sistema pediu, com uma folga de 1/8 (a tabela cresce
/// entre uma chamada e outra) — não para o dobro. Dobrar passava do teto numa
/// máquina com ~1 milhão de handles (~38 MB pedidos → 76 MB) e o snapshot
/// voltava vazio, mesmo cabendo. Sem dica de tamanho, aí sim dobra.
fn next_handle_snapshot_size(needed: usize, capacity: usize) -> Option<usize> {
    let grown = if needed > 0 {
        needed
            .saturating_add(needed / 8)
            .min(HANDLE_SNAPSHOT_MAX_BYTES)
            .max(needed)
    } else {
        capacity.saturating_mul(2)
    };
    (grown <= HANDLE_SNAPSHOT_MAX_BYTES).then_some(grown)
}

fn system_handle_snapshot() -> Vec<SystemHandleEntryEx> {
    let mut bytes: usize = 1 << 20;
    // O tamanho da tabela muda entre a consulta do tamanho e a leitura, então
    // o laço é limitado em vez de infinito.
    for _ in 0..8 {
        let words = bytes / 8 + 2;
        let mut buffer: Vec<u64> = Vec::new();
        if buffer.try_reserve_exact(words).is_err() {
            return Vec::new();
        }
        buffer.resize(words, 0);

        let capacity = words * 8;
        if capacity > u32::MAX as usize {
            return Vec::new();
        }
        let mut needed: u32 = 0;
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_EXTENDED_HANDLE_INFORMATION,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                capacity as u32,
                &mut needed,
            )
        };

        if status == STATUS_INFO_LENGTH_MISMATCH {
            match next_handle_snapshot_size(needed as usize, capacity) {
                Some(grown) => {
                    bytes = grown;
                    continue;
                }
                None => return Vec::new(),
            }
        }
        if status != 0 {
            return Vec::new();
        }
        return unsafe { parse_handle_snapshot(buffer.as_ptr() as *const u8, capacity) };
    }
    Vec::new()
}

/// Lê o cabeçalho (`NumberOfHandles`, `Reserved`) e as entradas seguintes.
///
/// # Safety
/// `base` precisa apontar para `bytes` válidos, alinhados em 8, devolvidos por
/// `NtQuerySystemInformation`.
unsafe fn parse_handle_snapshot(base: *const u8, bytes: usize) -> Vec<SystemHandleEntryEx> {
    let header = std::mem::size_of::<usize>() * 2;
    let entry_size = std::mem::size_of::<SystemHandleEntryEx>();
    if bytes < header + entry_size {
        return Vec::new();
    }
    let declared = std::ptr::read_unaligned(base as *const usize);
    // Nunca confie no contador: limite pelo que cabe no buffer.
    let count = declared.min((bytes - header) / entry_size);

    let mut entries: Vec<SystemHandleEntryEx> = Vec::new();
    if entries.try_reserve_exact(count).is_err() {
        return Vec::new();
    }
    for i in 0..count {
        let entry = base.add(header + i * entry_size) as *const SystemHandleEntryEx;
        entries.push(std::ptr::read_unaligned(entry));
    }
    entries
}

/// Tira a foto da tabela já sabendo qual é o índice do tipo "Event".
///
/// O índice é descoberto criando um evento anônimo nosso e procurando-o na
/// mesma foto — assim `NtQueryObject` só é chamado em handles de evento.
fn snapshot_with_event_type_index() -> Option<(Vec<SystemHandleEntryEx>, u16)> {
    let probe = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if probe.is_null() {
        return None;
    }
    let self_pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() } as usize;
    let probe_value = probe as usize;

    let snapshot = system_handle_snapshot();
    let index = snapshot
        .iter()
        .find(|e| e.unique_process_id == self_pid && e.handle_value == probe_value)
        .map(|e| e.object_type_index);

    unsafe { CloseHandle(probe) };
    index.map(|index| (snapshot, index))
}

/// Nome do objeto por trás de um handle **do nosso processo**. `None` em
/// qualquer falha ou objeto sem nome.
fn handle_object_name(handle: HANDLE) -> Option<String> {
    let mut buffer: Vec<u64> = vec![0; OBJECT_NAME_BUFFER_WORDS];
    let capacity = buffer.len() * 8;
    let mut needed: u32 = 0;
    let status = unsafe {
        NtQueryObject(
            handle,
            OBJECT_NAME_INFORMATION,
            buffer.as_mut_ptr() as *mut std::ffi::c_void,
            capacity as u32,
            &mut needed,
        )
    };
    if status != 0 {
        return None;
    }

    // OBJECT_NAME_INFORMATION = UNICODE_STRING { u16 Length; u16 MaximumLength;
    // <padding> u16* Buffer } seguido dos caracteres. O ponteiro fica a um
    // `usize` do início nos dois alvos (x86 e x64).
    let base = buffer.as_ptr() as *const u8;
    let (length, text) = unsafe {
        (
            std::ptr::read_unaligned(base as *const u16),
            std::ptr::read_unaligned(base.add(std::mem::size_of::<usize>()) as *const *const u16),
        )
    };
    if text.is_null() || length == 0 {
        return None;
    }
    let chars = length as usize / 2;
    // Sanidade: o nome tem que ter cabido no buffer que passamos.
    if chars > capacity / 2 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(text, chars) };
    Some(String::from_utf16_lossy(slice))
}

/// Resultado de uma varredura: handles fechados e processos do Roblox que nem
/// puderam ser abertos (`OpenProcess` negado — tipicamente um cliente rodando
/// como administrador, que um app sem elevação não alcança). Antes esses eram
/// pulados em silêncio.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SingletonCloseReport {
    pub closed: usize,
    pub unreachable: usize,
}

/// Fecha, dentro de `pid`, os handles de evento cujo nome termina em `suffix`.
/// Devolve quantos foram fechados, ou `None` se o processo nem pôde ser aberto.
fn close_singleton_events_in_process(
    pid: u32,
    suffix: &str,
    snapshot: &[SystemHandleEntryEx],
    event_type_index: u16,
) -> Option<usize> {
    let target = unsafe { OpenProcess(PROCESS_DUP_HANDLE, 0, pid) };
    if target.is_null() {
        return None;
    }
    let this_process = unsafe { GetCurrentProcess() };
    let pid_key = pid as usize;
    let mut closed = 0usize;

    for entry in snapshot
        .iter()
        .filter(|e| e.unique_process_id == pid_key && e.object_type_index == event_type_index)
    {
        let source = entry.handle_value as HANDLE;

        // 1ª duplicata: só para ler o nome.
        let mut probe: HANDLE = std::ptr::null_mut();
        let duplicated = unsafe {
            DuplicateHandle(
                target,
                source,
                this_process,
                &mut probe,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if duplicated == 0 {
            continue;
        }
        let name = handle_object_name(probe);
        unsafe { CloseHandle(probe) };

        let Some(name) = name else { continue };
        if !singleton_name_matches(&name, suffix) {
            continue;
        }

        // 2ª duplicata: fecha o original dentro do processo alvo.
        let mut sink: HANDLE = std::ptr::null_mut();
        let removed = unsafe {
            DuplicateHandle(
                target,
                source,
                this_process,
                &mut sink,
                0,
                0,
                DUPLICATE_CLOSE_SOURCE,
            )
        };
        if removed != 0 {
            unsafe { CloseHandle(sink) };
            closed += 1;
        }
    }

    unsafe { CloseHandle(target) };
    Some(closed)
}

/// Varre os `pids` indicados e fecha os eventos cujo nome termina em `suffix`.
fn close_singleton_events_for_pids(pids: &[u32], suffix: &str) -> SingletonCloseReport {
    let mut report = SingletonCloseReport::default();
    if pids.is_empty() || suffix.is_empty() {
        return report;
    }
    let Some((snapshot, event_type_index)) = snapshot_with_event_type_index() else {
        return report;
    };
    if snapshot.is_empty() {
        return report;
    }
    for pid in pids {
        match close_singleton_events_in_process(*pid, suffix, &snapshot, event_type_index) {
            Some(closed) => report.closed += closed,
            None => report.unreachable += 1,
        }
    }
    report
}

/// Carência de um cliente recém-aberto antes de fechar o handle dele: logo na
/// subida ele ainda está montando a instância única, e fechar o Event nessa
/// hora pode fazê-lo sair. Com o `AccountJoinDelay` padrão (8 s) isso quase
/// nunca espera.
const YOUNG_CLIENT_GRACE: Duration = Duration::from_secs(5);

/// Quanto esperar até o cliente mais novo (`created_ms`, hora de criação de
/// cada processo) ter `YOUNG_CLIENT_GRACE` de vida. Nunca mais que a carência.
fn singleton_close_wait(now_ms: i64, created_ms: &[Option<i64>]) -> Duration {
    let grace_ms = YOUNG_CLIENT_GRACE.as_millis() as i64;
    let wait_ms = created_ms
        .iter()
        .flatten()
        .map(|created| (grace_ms - (now_ms - created)).clamp(0, grace_ms))
        .max()
        .unwrap_or(0);
    Duration::from_millis(wait_ms as u64)
}

/// Fecha o `ROBLOX_singletonEvent` de todos os clientes Roblox abertos.
/// Devolve quantos handles foram fechados e quantos clientes não puderam ser
/// abertos para isso.
///
/// Os clientes **não** são fechados: eles seguem rodando, apenas deixam de
/// segurar o nome que faz a próxima instância desistir. É a etapa que permite
/// entrar com outras contas depois que o usuário já abriu o jogo pelo site.
pub fn close_roblox_singleton_handles() -> SingletonCloseReport {
    let pids = find_roblox_pids_all();
    if pids.is_empty() {
        return SingletonCloseReport::default();
    }
    let os = WindowsExternalClientOs;
    let created: Vec<Option<i64>> = pids.iter().map(|pid| os.process_created_ms(*pid)).collect();
    let wait = singleton_close_wait(os.now_ms(), &created);
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
    let report = close_singleton_events_for_pids(&pids, ROBLOX_SINGLETON_EVENT);
    if report.closed > 0 {
        eprintln!(
            "Multi Roblox: {} handle(s) de {} fechado(s) em {} processo(s) Roblox (clientes mantidos abertos)",
            report.closed,
            ROBLOX_SINGLETON_EVENT,
            pids.len()
        );
    }
    if report.unreachable > 0 {
        eprintln!(
            "Multi Roblox: {} processo(s) Roblox não puderam ser abertos para fechar o {} (rodando como administrador?)",
            report.unreachable, ROBLOX_SINGLETON_EVENT
        );
    }
    report
}

/// `OpenEventW` falhou (handle nulo) com `last_error`: esse nome segura um
/// Event que faz o próximo cliente desistir?
///
/// - `ERROR_FILE_NOT_FOUND`: o nome não existe — nada barra.
/// - `ERROR_INVALID_HANDLE`: o nome existe, mas como objeto de **outro tipo** —
///   um Mutex, a mesma técnica da nossa própria reserva (`core.rs`) e de outras
///   ferramentas de multi-instância. Um cliente do Roblox não cria o Event
///   sobre esse nome, então não há instância antiga para sinalizar: não barra.
///   (Antes qualquer erro que não fosse "não encontrado" contava como Event
///   existente, e um Mutex de outra ferramenta com esse nome fazia o app pular
///   todo segundo launch com "could not be unlocked".)
/// - `ERROR_ACCESS_DENIED`: existe um Event, de um cliente **elevado**, que não
///   conseguimos abrir — barra.
/// - qualquer outro erro: conservador, trata como Event existente — barra.
fn open_event_failure_blocks(last_error: u32) -> bool {
    !matches!(last_error, ERROR_FILE_NOT_FOUND | ERROR_INVALID_HANDLE)
}

/// Existe, nesta sessão, um `ROBLOX_singletonEvent` (um Event de verdade) que
/// faz o próximo cliente desistir? `OpenEventW` sem prefixo abre no mesmo
/// namespace em que o cliente do Roblox cria o dele. Um objeto de outro tipo
/// com o mesmo nome (um Mutex — a nossa reserva ou a de outra ferramenta) **não**
/// conta: ver [`open_event_failure_blocks`].
fn named_event_exists(name: &str) -> bool {
    let wide = encode_wide(name);
    let handle = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, wide.as_ptr()) };
    if handle.is_null() {
        return open_event_failure_blocks(unsafe { GetLastError() });
    }
    unsafe { CloseHandle(handle) };
    true
}
