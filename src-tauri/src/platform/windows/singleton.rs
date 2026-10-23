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
// - primeiro só processos cujo executável é do Roblox (`find_roblox_pids_all`);
//   se o nome ainda existir depois disso, outro programa (Fishtrap, executor)
//   o segura: `find_foreign_singleton_holders` só diz quem é, sem fechar nada;
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

/// Quem segura o objeto da nossa sonda (`probe_pid`/`probe_handle`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum SingletonHolders {
    /// A tabela traz o endereço do objeto: só os processos com handle nele.
    Exact(Vec<u32>),
    /// Endereço zerado (Windows 11 24H2 sem privilégio de depuração): todo
    /// processo com algum Event é candidato, e o nome decide em cada um.
    Candidates(Vec<u32>),
}

fn singleton_holders(
    snapshot: &[SystemHandleEntryEx],
    probe_pid: usize,
    probe_handle: usize,
    event_type_index: u16,
) -> SingletonHolders {
    let object = snapshot
        .iter()
        .find(|e| e.unique_process_id == probe_pid && e.handle_value == probe_handle)
        .map(|e| e.object)
        .unwrap_or(0);
    let mut pids: Vec<u32> = snapshot
        .iter()
        .filter(|e| e.object_type_index == event_type_index)
        .filter(|e| !(e.unique_process_id == probe_pid && e.handle_value == probe_handle))
        .filter(|e| object == 0 || e.object == object)
        .map(|e| e.unique_process_id as u32)
        .collect();
    pids.sort_unstable();
    pids.dedup();
    if object == 0 {
        // A sonda também é um Event do nosso processo: ele entra como candidato.
        if !pids.contains(&(probe_pid as u32)) {
            pids.push(probe_pid as u32);
            pids.sort_unstable();
        }
        SingletonHolders::Candidates(pids)
    } else {
        SingletonHolders::Exact(pids)
    }
}

/// `pid` tem um Event chamado exatamente `name`? Só **lê** o nome (uma cópia
/// do handle, fechada na hora): nada é fechado no outro processo. `None` se o
/// processo nem abre (administrador).
fn process_holds_named_event(
    pid: u32,
    name: &str,
    snapshot: &[SystemHandleEntryEx],
    event_type_index: u16,
) -> Option<bool> {
    let target = unsafe { OpenProcess(PROCESS_DUP_HANDLE, 0, pid) };
    if target.is_null() {
        return None;
    }
    let this_process = unsafe { GetCurrentProcess() };
    let pid_key = pid as usize;
    let mut found = false;
    for entry in snapshot
        .iter()
        .filter(|e| e.unique_process_id == pid_key && e.object_type_index == event_type_index)
    {
        let mut probe: HANDLE = std::ptr::null_mut();
        let duplicated = unsafe {
            DuplicateHandle(
                target,
                entry.handle_value as HANDLE,
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
        let matches = handle_object_name(probe).is_some_and(|n| singleton_name_matches(&n, name));
        unsafe { CloseHandle(probe) };
        if matches {
            found = true;
            break;
        }
    }
    unsafe { CloseHandle(target) };
    Some(found)
}

/// Quais processos (fora `skip_pids`) seguram o Event `name`. Só diagnóstico:
/// a trava **não** é liberada neles.
///
/// Ferramentas como o Fishtrap e executores (Real) seguram um handle do
/// `ROBLOX_singletonEvent`; enquanto ele existir, o próximo cliente desiste e
/// o app só consegue dizer quem é para a pessoa fechar. Fechar o handle no
/// outro programa resolveria, mas foi testado e descartado em 23/10/2026: a
/// edição completa passou a ser marcada por um motor de análise.
fn find_foreign_singleton_holders(name: &str, skip_pids: &[u32]) -> Vec<u32> {
    let wide = encode_wide(name);
    let probe = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, wide.as_ptr()) };
    if probe.is_null() {
        return Vec::new();
    }
    let self_pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() } as usize;
    let probe_value = probe as usize;
    let snapshot = snapshot_with_event_type_index();
    unsafe { CloseHandle(probe) };
    let Some((snapshot, event_type_index)) = snapshot else {
        return Vec::new();
    };
    match singleton_holders(&snapshot, self_pid, probe_value, event_type_index) {
        // A tabela já diz quem é: nem precisa abrir processo nenhum.
        SingletonHolders::Exact(pids) => {
            pids.into_iter().filter(|pid| !skip_pids.contains(pid)).collect()
        }
        SingletonHolders::Candidates(pids) => pids
            .into_iter()
            .filter(|pid| !skip_pids.contains(pid))
            .filter(|pid| {
                process_holds_named_event(*pid, name, &snapshot, event_type_index) == Some(true)
            })
            .collect(),
    }
}

/// Quem, fora os clientes do Roblox e o próprio app, segura o
/// `ROBLOX_singletonEvent`.
pub fn find_foreign_roblox_singleton_holders() -> Vec<u32> {
    let mut skip = find_roblox_pids_all();
    skip.push(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() });
    find_foreign_singleton_holders(ROBLOX_SINGLETON_EVENT, &skip)
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

#[cfg(test)]
mod singleton_event_tests {
    use super::*;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;

    #[test]
    fn a_large_handle_table_grows_to_what_was_asked_not_to_double() {
        // Medido numa maquina com ~1 milhao de handles: a tabela pede ~38 MB.
        // Dobrar 38 MB passava do teto de 64 MB e o snapshot desistia vazio.
        let mb = 1024 * 1024;
        let next = next_handle_snapshot_size(38 * mb, 38 * mb).expect("38 MB cabe no teto");
        assert!(next >= 38 * mb, "precisa caber o que foi pedido");
        assert!(next <= HANDLE_SNAPSHOT_MAX_BYTES);
    }

    #[test]
    fn a_machine_with_two_million_handles_still_gets_a_snapshot() {
        // Medido em 03/10/2026: um programa de periférico vazando handles
        // (NGenuity2Helper, 1,77 milhão) levou a tabela a ~77 MB, acima do teto
        // de 64 MB de então — o singleton do Roblox deixava de ser fechado.
        let mb = 1024 * 1024;
        assert!(next_handle_snapshot_size(80 * mb, 80 * mb).is_some());
    }

    #[test]
    fn the_snapshot_still_gives_up_past_the_ceiling() {
        assert_eq!(next_handle_snapshot_size(HANDLE_SNAPSHOT_MAX_BYTES + 1, 1024), None);
    }

    #[test]
    fn without_a_size_hint_the_snapshot_doubles() {
        assert_eq!(next_handle_snapshot_size(0, 1024 * 1024), Some(2 * 1024 * 1024));
    }

    #[test]
    fn a_wrong_type_object_of_the_same_name_does_not_block_the_next_client() {
        // Nome livre: `OpenEventW` dá ERROR_FILE_NOT_FOUND — nada barra.
        assert!(!open_event_failure_blocks(ERROR_FILE_NOT_FOUND));
        // Nome ocupado por um objeto de outro tipo (um Mutex: a nossa reserva ou
        // a de outra ferramenta de multi-instância): ERROR_INVALID_HANDLE. Não é
        // um Event de cliente, então não barra — antes barrava e o app pulava
        // todo segundo launch.
        assert!(!open_event_failure_blocks(ERROR_INVALID_HANDLE));
        // Event de um cliente elevado que não conseguimos abrir: barra.
        assert!(open_event_failure_blocks(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED));
        // Qualquer outro erro: conservador, trata como Event existente.
        assert!(open_event_failure_blocks(1_u32)); // ERROR_INVALID_FUNCTION
    }

    struct OwnedEvent(HANDLE);

    impl Drop for OwnedEvent {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    impl OwnedEvent {
        /// Marca o handle como já fechado (o teste fechou de fora, via
        /// `DUPLICATE_CLOSE_SOURCE`) para o `Drop` não fechar duas vezes.
        fn forget(&mut self) {
            self.0 = std::ptr::null_mut();
        }
    }

    fn create_named_event(name: &str) -> Option<OwnedEvent> {
        let wide = encode_wide(name);
        let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, wide.as_ptr()) };
        (!handle.is_null()).then_some(OwnedEvent(handle))
    }

    /// A checagem de existência é a de produção (`super::named_event_exists`):
    /// é ela que decide se o launch segue com o Event já fechado.
    #[test]
    fn an_event_exists_only_while_someone_holds_it() {
        let name = format!("RAMTest_existsProbe_{}", self_pid());
        assert!(!named_event_exists(&name), "o nome já existia antes do teste");
        let event = create_named_event(&name).expect("CreateEventW falhou");
        assert!(named_event_exists(&name));
        drop(event);
        assert!(!named_event_exists(&name), "o nome sobreviveu ao último handle");
    }

    fn self_pid() -> u32 {
        unsafe { GetCurrentProcessId() }
    }

    #[test]
    fn matching_accepts_the_real_name_in_any_case() {
        assert!(singleton_name_matches(
            "\\Sessions\\1\\BaseNamedObjects\\ROBLOX_singletonEvent",
            ROBLOX_SINGLETON_EVENT
        ));
        assert!(singleton_name_matches(
            "\\Sessions\\1\\BaseNamedObjects\\roblox_singletonevent",
            ROBLOX_SINGLETON_EVENT
        ));
        assert!(singleton_name_matches(
            "\\SESSIONS\\2\\BASENAMEDOBJECTS\\RoBlOx_SiNgLeToNeVeNt",
            ROBLOX_SINGLETON_EVENT
        ));
        // Sem namespace nenhum ainda é o objeto certo.
        assert!(singleton_name_matches(
            ROBLOX_SINGLETON_EVENT,
            ROBLOX_SINGLETON_EVENT
        ));
    }

    #[test]
    fn matching_rejects_names_that_only_look_alike() {
        for name in [
            "",
            "\\Sessions\\1\\BaseNamedObjects\\ROBLOX_singletonMutex",
            "\\Sessions\\1\\BaseNamedObjects\\ROBLOX_singletonEventX",
            "\\Sessions\\1\\BaseNamedObjects\\ROBLOX_singleton",
            // Termina com o sufixo, mas não na fronteira do namespace.
            "\\Sessions\\1\\BaseNamedObjects\\FAKE_ROBLOX_singletonEvent",
            "\\Sessions\\1\\BaseNamedObjects\\ROBLOX_singletonEvent\\child",
            "singletonEvent",
        ] {
            assert!(
                !singleton_name_matches(name, ROBLOX_SINGLETON_EVENT),
                "não devia casar: {name:?}"
            );
        }
    }

    #[test]
    fn matching_rejects_an_empty_suffix() {
        assert!(!singleton_name_matches("", ""));
        assert!(!singleton_name_matches(
            "\\Sessions\\1\\BaseNamedObjects\\ROBLOX_singletonEvent",
            ""
        ));
    }

    #[test]
    fn the_handle_snapshot_is_readable_and_lists_this_process() {
        let snapshot = system_handle_snapshot();
        assert!(
            !snapshot.is_empty(),
            "NtQuerySystemInformation não devolveu handle nenhum"
        );
        let me = self_pid() as usize;
        assert!(
            snapshot.iter().any(|e| e.unique_process_id == me),
            "o próprio processo de teste deveria aparecer na tabela"
        );
    }

    #[test]
    fn the_event_type_index_is_discovered_and_matches_one_of_our_events() {
        let event = create_named_event("RAMTest_typeIndexProbe_7c21").expect("CreateEventW falhou");
        let (snapshot, event_type_index) =
            snapshot_with_event_type_index().expect("não achei o índice do tipo Event");

        let me = self_pid() as usize;
        let value = event.0 as usize;
        let entry = snapshot
            .iter()
            .find(|e| e.unique_process_id == me && e.handle_value == value)
            .expect("o evento criado não apareceu na tabela de handles");
        assert_eq!(
            entry.object_type_index, event_type_index,
            "o índice descoberto não bate com o do nosso evento nomeado"
        );
    }

    #[test]
    fn a_named_event_of_ours_is_found_closed_and_disappears() {
        // Ciclo completo sem depender do Roblox: criamos o evento, mandamos
        // fechar pelo mesmo caminho usado nos clientes e conferimos que o nome
        // some do namespace.
        let name = "RAMTest_cycle_singletonEvent";
        let mut event = create_named_event(name).expect("CreateEventW falhou");
        assert!(named_event_exists(name), "o evento devia existir antes");

        let closed = close_singleton_events_for_pids(&[self_pid()], name).closed;
        assert_eq!(closed, 1, "devia ter fechado exatamente um handle");
        event.forget();

        assert!(
            !named_event_exists(name),
            "o nome devia ter sumido depois do DUPLICATE_CLOSE_SOURCE"
        );
    }

    #[test]
    fn closing_leaves_similar_names_untouched() {
        let target = "RAMTest_similar_singletonEvent";
        let sibling = "RAMTest_similar_singletonEventX";
        // Termina com o sufixo, mas fora da fronteira do namespace.
        let prefixed = "XRAMTest_similar_singletonEvent";
        let other = "RAMTest_similar_otherEvent";

        let mut target_event = create_named_event(target).expect("CreateEventW falhou");
        let _sibling_event = create_named_event(sibling).expect("CreateEventW falhou");
        let _prefixed_event = create_named_event(prefixed).expect("CreateEventW falhou");
        let _other_event = create_named_event(other).expect("CreateEventW falhou");

        let closed = close_singleton_events_for_pids(&[self_pid()], target).closed;
        assert_eq!(closed, 1, "só o nome exato podia ter sido fechado");
        target_event.forget();

        assert!(!named_event_exists(target));
        assert!(
            named_event_exists(sibling),
            "um nome que apenas começa igual não pode ser fechado"
        );
        assert!(
            named_event_exists(prefixed),
            "um nome que termina com o sufixo fora da fronteira do namespace não pode ser fechado"
        );
        assert!(
            named_event_exists(other),
            "um evento de nome diferente não pode ser fechado"
        );
    }

    #[test]
    fn events_of_processes_that_are_not_roblox_are_never_touched() {
        // O binário de teste não é um executável do Roblox, então a varredura
        // restrita aos pids do Roblox não pode enxergar os nossos handles.
        let me = self_pid();
        assert!(
            !find_roblox_pids_all().contains(&me),
            "o runner de teste não pode ser confundido com um cliente Roblox"
        );

        let name = "RAMTest_foreign_singletonEvent";
        let _event = create_named_event(name).expect("CreateEventW falhou");

        let roblox_pids = find_roblox_pids_all();
        let closed = close_singleton_events_for_pids(&roblox_pids, name).closed;
        assert_eq!(
            closed, 0,
            "nenhum processo Roblox segura um evento com esse nome"
        );
        assert!(
            named_event_exists(name),
            "o evento de um processo que não é do Roblox tem que continuar aberto"
        );
    }

    #[test]
    fn closing_is_a_no_op_without_pids_or_suffix() {
        assert_eq!(
            close_singleton_events_for_pids(&[], ROBLOX_SINGLETON_EVENT),
            SingletonCloseReport::default()
        );
        assert_eq!(
            close_singleton_events_for_pids(&[self_pid()], ""),
            SingletonCloseReport::default()
        );
    }

    // ── Trava presa em outro programa (tickets de 22/10/2026) ──────────────
    //
    // Com o Fishtrap ou o executor Real abertos, o launch da 2ª conta dava
    // "Another Roblox window could not be unlocked" até reiniciar o PC: um
    // processo que não é do Roblox segurava o `ROBLOX_singletonEvent`, e a
    // varredura só olhava os executáveis do Roblox.

    fn entry(pid: usize, handle: usize, object: usize, type_index: u16) -> SystemHandleEntryEx {
        SystemHandleEntryEx {
            object,
            unique_process_id: pid,
            handle_value: handle,
            granted_access: 0,
            creator_back_trace_index: 0,
            object_type_index: type_index,
            handle_attributes: 0,
            reserved: 0,
        }
    }

    #[test]
    fn holders_are_the_processes_that_share_the_object_of_our_probe() {
        const EVENT: u16 = 16;
        let snapshot = vec![
            entry(10, 0x40, 0xAAAA, EVENT), // nossa sonda
            entry(20, 0x88, 0xAAAA, EVENT), // outro programa com a mesma trava
            entry(30, 0x90, 0xBBBB, EVENT), // outro evento qualquer
            entry(40, 0x10, 0xAAAA, EVENT),
            entry(20, 0x99, 0xAAAA, EVENT), // dois handles no mesmo processo
        ];
        assert_eq!(
            singleton_holders(&snapshot, 10, 0x40, EVENT),
            SingletonHolders::Exact(vec![20, 40])
        );
    }

    #[test]
    fn without_object_addresses_every_process_with_events_is_a_candidate() {
        // Windows 11 24H2 sem privilégio de depuração zera o endereço do
        // objeto: aí só dá para olhar o nome em cada processo com Event.
        const EVENT: u16 = 16;
        let snapshot = vec![
            entry(10, 0x40, 0, EVENT),
            entry(20, 0x88, 0, EVENT),
            entry(20, 0x89, 0, EVENT),
            entry(30, 0x90, 0, 7), // não é Event
            entry(40, 0x10, 0, EVENT),
        ];
        assert_eq!(
            singleton_holders(&snapshot, 10, 0x40, EVENT),
            SingletonHolders::Candidates(vec![10, 20, 40])
        );
    }

    #[test]
    fn an_event_held_by_a_process_that_is_not_roblox_is_found_and_left_alone() {
        let name = "RAMTest_foreignHolder_singletonEvent";
        let _event = create_named_event(name).expect("CreateEventW falhou");
        // Pulando o próprio processo, ninguém segura.
        assert!(find_foreign_singleton_holders(name, &[self_pid()]).is_empty());
        // Sem pular, o "outro programa" (este runner) aparece...
        assert_eq!(find_foreign_singleton_holders(name, &[]), vec![self_pid()]);
        // ...e a trava continua com ele: só diagnóstico.
        assert!(named_event_exists(name), "nada pode ser fechado no outro programa");
    }

    #[test]
    fn no_event_no_holder() {
        assert!(find_foreign_singleton_holders("RAMTest_absent_singletonEvent", &[]).is_empty());
    }

    /// Processo que não abre (Roblox como administrador, ou já saiu) era
    /// pulado em silêncio; agora entra na conta, para a mensagem dizer o motivo.
    #[test]
    fn a_process_that_cannot_be_opened_is_counted() {
        // pid 0 (System Idle) nunca abre com PROCESS_DUP_HANDLE.
        let report = close_singleton_events_for_pids(&[0], "RAMTest_unreachable_singletonEvent");
        assert_eq!(report.closed, 0);
        assert_eq!(report.unreachable, 1);
    }
}
