struct EnumWindowData {
    target_pid: u32,
    result_hwnd: HWND,
}

unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: isize) -> i32 {
    let data = &mut *(lparam as *mut EnumWindowData);
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == data.target_pid && IsWindowVisible(hwnd) != 0 {
        data.result_hwnd = hwnd;
        return 0;
    }
    1
}

pub fn find_main_window(target_pid: u32) -> Option<HWND> {
    let mut data = EnumWindowData {
        target_pid,
        result_hwnd: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(Some(enum_window_callback), &mut data as *mut _ as isize);
    }
    if !data.result_hwnd.is_null() {
        Some(data.result_hwnd)
    } else {
        None
    }
}

pub fn get_window_position(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) != 0 {
            Some((
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
            ))
        } else {
            None
        }
    }
}

pub fn set_window_position(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> bool {
    unsafe { MoveWindow(hwnd, x, y, w, h, 1) != 0 }
}

pub fn minimize_window(hwnd: HWND) -> bool {
    unsafe { ShowWindow(hwnd, SW_MINIMIZE) != 0 }
}

/// A janela está minimizada? O AFK mode pergunta antes de trazer a janela para
/// frente, para devolvê-la ao estado em que o usuário a deixou.
pub fn window_is_minimized(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        return false;
    }
    unsafe { IsIconic(hwnd) != 0 }
}

/// Para que a janela vem para frente — é isso que decide se o estado dela
/// (minimizada, normal, maximizada) pode mudar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusPurpose {
    /// Trazer uma janela para frente para ela receber algo: o cliente da conta
    /// no AFK mode, o "focar a janela" da conta. Minimizada é restaurada (não dá
    /// para receber tecla minimizada); normal ou maximizada fica como está.
    Bring,
    /// Devolver o primeiro plano à janela que estava lá antes — a janela em que
    /// o usuário estava trabalhando. Nunca mexe no estado dela.
    GiveBack,
}

/// O `ShowWindow` que vai antes do `SetForegroundWindow`, se algum.
///
/// `SW_RESTORE` só em janela **minimizada**: numa maximizada ele a devolve ao
/// tamanho normal. Aplicado sempre, o AFK mode tirava do maximizado o cliente
/// alvo e — ao devolver o foco — a janela em que o usuário estava trabalhando,
/// a cada ciclo, até quando o foco tinha sido negado e nada foi enviado.
fn show_command_before_focus(purpose: FocusPurpose, minimized: bool) -> Option<i32> {
    match purpose {
        FocusPurpose::Bring if minimized => Some(SW_RESTORE),
        FocusPurpose::Bring | FocusPurpose::GiveBack => None,
    }
}

fn bring_to_foreground(hwnd: HWND, purpose: FocusPurpose) -> bool {
    if let Some(command) = show_command_before_focus(purpose, window_is_minimized(hwnd)) {
        unsafe {
            let _ = ShowWindow(hwnd, command);
        }
    }
    unsafe { SetForegroundWindow(hwnd) != 0 }
}

/// Traz a janela para o primeiro plano. Minimizada volta ao tamanho de antes;
/// normal ou maximizada vem como está.
pub fn focus_window(hwnd: HWND) -> bool {
    bring_to_foreground(hwnd, FocusPurpose::Bring)
}

/// Devolve o primeiro plano à janela que estava lá, **sem mexer no estado
/// dela**: nem restaurar, nem desmaximizar.
pub fn give_focus_back(hwnd: HWND) -> bool {
    bring_to_foreground(hwnd, FocusPurpose::GiveBack)
}

fn wait_for_process_exit(pid: u32, timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if !get_roblox_pids().contains(&pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    !get_roblox_pids().contains(&pid)
}

fn is_roblox_pid_alive(pid: u32) -> bool {
    get_roblox_pids().contains(&pid)
}

pub fn get_foreground_hwnd() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let read = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if read > 0 {
            String::from_utf16_lossy(&buf[..read as usize])
        } else {
            String::new()
        }
    }
}

// ── Grid layout of Roblox windows across monitors ──────────────────────────
//
// Ported from the Robeats "calibração" grid feature: enumerate physical
// monitors, normalize every Roblox window to the most common size, then place
// them cell-by-cell (left→right, top→bottom) across the selected monitors with
// a gap. A cell is only used if the window fits entirely inside the monitor;
// when a monitor runs out of cells, the next one is used. Fixed-size style —
// leftover windows stay unplaced.

#[derive(Clone, Serialize)]
pub struct MonitorInfoDto {
    pub index: usize, // 1-based, in left→top order
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
}

#[derive(Clone, Copy)]
pub struct RobloxWin {
    pub hwnd: isize,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy)]
struct GridCell {
    x: i32,
    y: i32,
}

struct MonitorCollect {
    list: Vec<(RECT, bool)>,
}

unsafe extern "system" fn monitor_enum_cb(
    hmon: HMONITOR,
    _hdc: HDC,
    _rc: *mut RECT,
    lparam: isize,
) -> i32 {
    let data = &mut *(lparam as *mut MonitorCollect);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(hmon, &mut mi) != 0 {
        let primary = (mi.dwFlags & MONITORINFOF_PRIMARY) != 0;
        // Use the work area (rcWork) so windows don't hide behind the taskbar.
        data.list.push((mi.rcWork, primary));
    }
    1
}

/// Physical monitors (work areas), ordered left→top, 1-based index.
pub fn list_monitors() -> Vec<MonitorInfoDto> {
    let mut data = MonitorCollect { list: Vec::new() };
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(monitor_enum_cb),
            &mut data as *mut _ as isize,
        );
    }
    data.list.sort_by_key(|(r, _)| (r.left, r.top));
    data.list
        .into_iter()
        .enumerate()
        .map(|(i, (r, primary))| MonitorInfoDto {
            index: i + 1,
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
            primary,
        })
        .collect()
}

struct RobloxWinCollect {
    pids: std::collections::HashSet<u32>,
    out: Vec<RobloxWin>,
}

unsafe extern "system" fn roblox_win_cb(hwnd: HWND, lparam: isize) -> i32 {
    let data = &mut *(lparam as *mut RobloxWinCollect);
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
        return 1;
    }
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if !data.pids.contains(&pid) {
        return 1;
    }
    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(hwnd, &mut rect) == 0 {
        return 1;
    }
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;
    if w < 200 || h < 200 {
        return 1;
    }
    data.out.push(RobloxWin {
        hwnd: hwnd as isize,
        x: rect.left,
        y: rect.top,
        w,
        h,
    });
    1
}

/// Visible, non-minimized Roblox windows, ordered top→bottom, left→right.
pub fn list_roblox_windows() -> Vec<RobloxWin> {
    let pids: std::collections::HashSet<u32> = get_roblox_pids().into_iter().collect();
    let mut data = RobloxWinCollect {
        pids,
        out: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(roblox_win_cb), &mut data as *mut _ as isize);
    }
    data.out.sort_by_key(|w| (w.y, w.x));
    data.out
}

/// Most frequent (width, height); ties broken by largest area.
fn most_common_size(wins: &[RobloxWin]) -> (i32, i32) {
    use std::collections::HashMap as Map;
    let mut counts: Map<(i32, i32), usize> = Map::new();
    for w in wins {
        *counts.entry((w.w, w.h)).or_insert(0) += 1;
    }
    let top = counts.values().copied().max().unwrap_or(0);
    counts
        .into_iter()
        .filter(|(_, c)| *c == top)
        .map(|(s, _)| s)
        .max_by_key(|(w, h)| (*w as i64) * (*h as i64))
        .unwrap_or((800, 600))
}

/// Distribute `count` windows of `win_w`×`win_h` into cells across `monitors`.
fn compute_grid(
    monitors: &[MonitorInfoDto],
    win_w: i32,
    win_h: i32,
    count: usize,
    gap: i32,
) -> Vec<GridCell> {
    let mut cells: Vec<GridCell> = Vec::new();
    for mon in monitors {
        let mut row = 0;
        while cells.len() < count {
            let cell_y = mon.y + gap + row * (win_h + gap);
            if cell_y + win_h > mon.y + mon.height {
                break; // row doesn't fit vertically -> monitor exhausted
            }
            let mut col = 0;
            let mut placed_in_row = false;
            while cells.len() < count {
                let cell_x = mon.x + gap + col * (win_w + gap);
                if cell_x + win_w > mon.x + mon.width {
                    break; // column doesn't fit -> end of row
                }
                cells.push(GridCell { x: cell_x, y: cell_y });
                placed_in_row = true;
                col += 1;
            }
            if !placed_in_row {
                break; // not even the first column fits -> monitor useless for this size
            }
            row += 1;
        }
    }
    cells.truncate(count);
    cells
}

/// Arrange all open Roblox windows into a grid across the selected monitors
/// (1-based indices; empty = all). Returns (arranged, total_windows).
pub fn arrange_roblox_grid(monitor_indices: &[usize], gap: i32) -> Result<(usize, usize), String> {
    let wins = list_roblox_windows();
    if wins.is_empty() {
        return Err("Nenhuma janela de Roblox aberta foi encontrada.".to_string());
    }
    let monitors_all = list_monitors();
    if monitors_all.is_empty() {
        return Err("Nenhum monitor foi detectado.".to_string());
    }
    let sel: Vec<usize> = if monitor_indices.is_empty() {
        (1..=monitors_all.len()).collect()
    } else {
        monitor_indices.to_vec()
    };
    let monitors: Vec<MonitorInfoDto> = sel
        .iter()
        .filter_map(|&i| i.checked_sub(1).and_then(|idx| monitors_all.get(idx)).cloned())
        .collect();
    if monitors.is_empty() {
        return Err("Selecione ao menos um monitor válido.".to_string());
    }
    let (mode_w, mode_h) = most_common_size(&wins);
    let gap = gap.max(0);
    let cells = compute_grid(&monitors, mode_w, mode_h, wins.len(), gap);
    if cells.is_empty() {
        return Err("Nenhuma célula coube nos monitores (janelas grandes demais para o gap?).".to_string());
    }
    let mut arranged = 0;
    for (w, cell) in wins.iter().zip(cells.iter()) {
        unsafe {
            let _ = ShowWindow(w.hwnd as HWND, SW_RESTORE);
        }
        if set_window_position(w.hwnd as HWND, cell.x, cell.y, mode_w, mode_h) {
            arranged += 1;
        }
    }
    Ok((arranged, wins.len()))
}

pub fn get_process_memory_mb(pid: u32) -> Option<u64> {
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    extern "system" {
        fn K32GetProcessMemoryInfo(
            process: HANDLE,
            ppsmemcounters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut counters: ProcessMemoryCounters = std::mem::zeroed();
        counters.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
        let result = K32GetProcessMemoryInfo(
            handle,
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        );
        CloseHandle(handle);
        if result != 0 {
            Some(counters.working_set_size as u64 / 1024 / 1024)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod win_windowing_tests {
    use super::*;

    // Only the pure geometry is covered: `most_common_size` and `compute_grid`
    // together are everything `arrange_roblox_grid` decides before it calls
    // MoveWindow. Enumeration and window moving need a live desktop.

    fn win(x: i32, y: i32, w: i32, h: i32) -> RobloxWin {
        RobloxWin {
            hwnd: 0,
            x,
            y,
            w,
            h,
        }
    }

    fn monitor(index: usize, x: i32, y: i32, width: i32, height: i32) -> MonitorInfoDto {
        MonitorInfoDto {
            index,
            x,
            y,
            width,
            height,
            primary: index == 1,
        }
    }

    // ── most_common_size ───────────────────────────────────────────────────

    #[test]
    fn most_common_size_picks_the_modal_window_size() {
        let wins = vec![
            win(0, 0, 800, 600),
            win(0, 0, 800, 600),
            win(0, 0, 1280, 720),
        ];
        assert_eq!(most_common_size(&wins), (800, 600));
    }

    #[test]
    fn most_common_size_breaks_ties_with_the_largest_area() {
        let wins = vec![
            win(0, 0, 800, 600),
            win(0, 0, 1280, 720),
            win(0, 0, 640, 480),
        ];
        assert_eq!(most_common_size(&wins), (1280, 720));
    }

    #[test]
    fn most_common_size_falls_back_to_800x600_for_no_windows() {
        assert_eq!(most_common_size(&[]), (800, 600));
    }

    #[test]
    fn most_common_size_returns_the_only_size_when_all_windows_match() {
        let wins = vec![win(0, 0, 1920, 1080); 5];
        assert_eq!(most_common_size(&wins), (1920, 1080));
    }

    // ── compute_grid ───────────────────────────────────────────────────────

    #[test]
    fn compute_grid_lays_cells_out_left_to_right_then_top_to_bottom() {
        // 1000x1000 monitor, 400x400 windows, gap 10:
        // x: 10, 420 (830 + 400 > 1000 -> row ends)
        // y: 10, 420 (830 + 400 > 1000 -> monitor exhausted)
        let monitors = vec![monitor(1, 0, 0, 1000, 1000)];
        let cells = compute_grid(&monitors, 400, 400, 4, 10);

        assert_eq!(cells.len(), 4);
        assert_eq!((cells[0].x, cells[0].y), (10, 10));
        assert_eq!((cells[1].x, cells[1].y), (420, 10));
        assert_eq!((cells[2].x, cells[2].y), (10, 420));
        assert_eq!((cells[3].x, cells[3].y), (420, 420));
    }

    #[test]
    fn compute_grid_never_returns_more_cells_than_requested() {
        let monitors = vec![monitor(1, 0, 0, 1000, 1000)];
        for count in 0..=4 {
            assert_eq!(compute_grid(&monitors, 400, 400, count, 10).len(), count);
        }
    }

    #[test]
    fn compute_grid_stops_when_the_monitor_runs_out_of_room() {
        // Only 4 cells fit; asking for 10 yields 4.
        let monitors = vec![monitor(1, 0, 0, 1000, 1000)];
        assert_eq!(compute_grid(&monitors, 400, 400, 10, 10).len(), 4);
    }

    #[test]
    fn compute_grid_honours_the_monitor_origin() {
        let monitors = vec![monitor(1, -1920, 120, 1000, 1000)];
        let cells = compute_grid(&monitors, 400, 400, 2, 10);
        assert_eq!((cells[0].x, cells[0].y), (-1910, 130));
        assert_eq!((cells[1].x, cells[1].y), (-1500, 130));
    }

    #[test]
    fn compute_grid_continues_onto_the_next_monitor() {
        // First monitor fits exactly one cell, second takes the remainder.
        let monitors = vec![
            monitor(1, 0, 0, 500, 500),
            monitor(2, 2000, 0, 1000, 1000),
        ];
        let cells = compute_grid(&monitors, 400, 400, 3, 10);

        assert_eq!(cells.len(), 3);
        assert_eq!((cells[0].x, cells[0].y), (10, 10));
        assert_eq!((cells[1].x, cells[1].y), (2010, 10));
        assert_eq!((cells[2].x, cells[2].y), (2420, 10));
    }

    #[test]
    fn compute_grid_skips_a_monitor_too_small_for_even_one_window() {
        let monitors = vec![
            monitor(1, 0, 0, 200, 200), // 400x400 cannot fit
            monitor(2, 1000, 0, 1000, 1000),
        ];
        let cells = compute_grid(&monitors, 400, 400, 2, 10);
        assert_eq!(cells.len(), 2);
        assert_eq!((cells[0].x, cells[0].y), (1010, 10));
        assert_eq!((cells[1].x, cells[1].y), (1420, 10));
    }

    #[test]
    fn compute_grid_returns_nothing_when_the_windows_are_larger_than_every_monitor() {
        let monitors = vec![monitor(1, 0, 0, 800, 600)];
        assert!(compute_grid(&monitors, 1920, 1080, 4, 10).is_empty());
    }

    #[test]
    fn compute_grid_returns_nothing_without_monitors() {
        assert!(compute_grid(&[], 400, 400, 4, 10).is_empty());
    }

    #[test]
    fn compute_grid_with_a_zero_gap_still_leaves_no_overlap() {
        let monitors = vec![monitor(1, 0, 0, 800, 800)];
        let cells = compute_grid(&monitors, 400, 400, 4, 0);
        assert_eq!(cells.len(), 4);
        assert_eq!((cells[0].x, cells[0].y), (0, 0));
        assert_eq!((cells[1].x, cells[1].y), (400, 0));
        assert_eq!((cells[2].x, cells[2].y), (0, 400));
        assert_eq!((cells[3].x, cells[3].y), (400, 400));

        // No two cells of the same size may overlap.
        for (i, a) in cells.iter().enumerate() {
            for b in cells.iter().skip(i + 1) {
                let overlaps_x = a.x < b.x + 400 && b.x < a.x + 400;
                let overlaps_y = a.y < b.y + 400 && b.y < a.y + 400;
                assert!(!(overlaps_x && overlaps_y), "cells overlap");
            }
        }
    }

    #[test]
    fn a_larger_gap_fits_fewer_cells() {
        let monitors = vec![monitor(1, 0, 0, 1000, 1000)];
        let small_gap = compute_grid(&monitors, 400, 400, 16, 10).len();
        let large_gap = compute_grid(&monitors, 400, 400, 16, 150).len();
        assert_eq!(small_gap, 4);
        assert!(
            large_gap < small_gap,
            "a bigger gap should fit fewer cells ({} vs {})",
            large_gap,
            small_gap
        );
    }

    #[test]
    fn every_cell_stays_inside_its_monitor_work_area() {
        let monitors = vec![
            monitor(1, 0, 0, 1920, 1040),
            monitor(2, 1920, -200, 2560, 1400),
        ];
        let (w, h) = (640, 360);
        let cells = compute_grid(&monitors, w, h, 40, 12);
        assert!(!cells.is_empty());

        for cell in &cells {
            let inside = monitors.iter().any(|m| {
                cell.x >= m.x
                    && cell.y >= m.y
                    && cell.x + w <= m.x + m.width
                    && cell.y + h <= m.y + m.height
            });
            assert!(inside, "cell ({}, {}) escaped every monitor", cell.x, cell.y);
        }
    }

    #[test]
    fn monitor_info_dto_serializes_its_fields() {
        let json = serde_json::to_value(monitor(1, -100, 0, 1920, 1080)).unwrap();
        assert_eq!(json["index"], 1);
        assert_eq!(json["x"], -100);
        assert_eq!(json["width"], 1920);
        assert_eq!(json["height"], 1080);
        assert_eq!(json["primary"], true);
    }
}

#[cfg(test)]
mod win_focus_tests {
    use super::*;

    // Só a decisão é coberta: `ShowWindow` e `SetForegroundWindow` de verdade
    // precisam de janela e de desktop.

    #[test]
    fn a_minimized_window_is_restored_before_it_comes_to_the_front() {
        assert_eq!(
            show_command_before_focus(FocusPurpose::Bring, true),
            Some(SW_RESTORE)
        );
    }

    /// `SW_RESTORE` numa janela maximizada a devolve ao tamanho normal: era o
    /// AFK mode tirando do maximizado o cliente alvo a cada ciclo.
    #[test]
    fn a_maximized_or_normal_window_comes_to_the_front_as_it_is() {
        assert_eq!(show_command_before_focus(FocusPurpose::Bring, false), None);
    }

    /// Devolver o foco é devolver a janela em que o usuário estava trabalhando:
    /// nem desmaximizar, nem restaurar — nem quando o foco tinha sido negado e
    /// nada foi enviado.
    #[test]
    fn giving_the_focus_back_never_changes_the_window_state() {
        assert_eq!(show_command_before_focus(FocusPurpose::GiveBack, false), None);
        assert_eq!(show_command_before_focus(FocusPurpose::GiveBack, true), None);
    }
}
