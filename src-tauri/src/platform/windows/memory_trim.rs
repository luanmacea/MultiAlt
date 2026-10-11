// Teto de memória (commands/memory_ceiling.rs): o único ponto que pede ao
// Windows para tirar da RAM as páginas que um cliente não está usando agora.
// Elas vão para o arquivo de paginação e voltam quando o cliente precisar —
// nada é fechado nem perdido.
//
// Só existe com a feature `memory-trim` (edição completa): é uma API nativa
// nova no binário (`K32EmptyWorkingSet`, do kernel32 — a mesma família do
// `K32GetProcessMemoryInfo` que o Watcher já usa). Sem a feature, a função
// devolve `false` e o teto fica desligado.

/// Pede ao Windows para liberar a memória do processo `pid`. Devolve se o
/// Windows aceitou.
#[cfg(feature = "memory-trim")]
pub fn trim_working_set(pid: u32) -> bool {
    use windows_sys::Win32::System::Threading::{
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
    };

    extern "system" {
        fn K32EmptyWorkingSet(process: HANDLE) -> i32;
    }

    if pid == 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA, 0, pid);
        if handle.is_null() {
            return false;
        }
        let ok = K32EmptyWorkingSet(handle) != 0;
        CloseHandle(handle);
        ok
    }
}

#[cfg(not(feature = "memory-trim"))]
pub fn trim_working_set(_pid: u32) -> bool {
    false
}
