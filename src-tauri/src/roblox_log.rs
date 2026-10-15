// Leitura de linhas do log do cliente Roblox (`%LOCALAPPDATA%\Roblox\logs`),
// compartilhada entre a varredura de clientes abertos pelo site
// (platform/windows/external_clients.rs) e a saúde dos clientes que o app
// abriu (commands/client_health.rs).
//
// Tudo aqui é puro: recebe a linha e devolve o pedaço que interessa. Nada
// guarda a linha inteira — ela pode trazer ticket, IP e Job ID.
//
// Formatos calibrados com os logs do cliente 0.743 da máquina do dono
// (15/10/2026), um por entrada em jogo, nesta ordem:
//
// ```text
// ... [FLog::Output] ! Joining game '<job>' place <place> at 10.x.x.x
// ... [FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:<place>, ..., userid:<conta>,
// ```

/// Marcador da linha que diz com que conta o cliente entrou no jogo.
pub const GAME_JOIN_LOAD_TIME_MARKER: &str = "[FLog::GameJoinLoadTime]";

/// Dígitos logo depois de `marker` (o primeiro, se o marcador repetir). Zero
/// ou nada de dígitos: `None`.
pub fn digits_after(line: &str, marker: &str) -> Option<i64> {
    let start = line.find(marker)? + marker.len();
    let digits: String = line[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let value: i64 = digits.parse().ok()?;
    (value > 0).then_some(value)
}

/// A conta com que o cliente entrou no jogo: `userid:` da linha
/// `[FLog::GameJoinLoadTime]` (o teleporte repete a linha; vale a última).
pub fn game_join_user_id(line: &str) -> Option<i64> {
    if !line.contains(GAME_JOIN_LOAD_TIME_MARKER) {
        return None;
    }
    digits_after(line, "userid:")
}

/// O `placeid:` da mesma linha `[FLog::GameJoinLoadTime]`.
pub fn game_join_place_id(line: &str) -> Option<i64> {
    if !line.contains(GAME_JOIN_LOAD_TIME_MARKER) {
        return None;
    }
    digits_after(line, "placeid:")
}

#[cfg(test)]
mod roblox_log_line_tests {
    use super::*;

    // Linhas no formato do cliente 0.743 (logs do dono, 15/10/2026), com ids,
    // Job IDs e IPs internos trocados por falsos.
    const PREFIX: &str = "2026-10-15T02:57:34.478Z,1.478033,6c20,6";

    fn line(body: &str) -> String {
        format!("{PREFIX} {body}")
    }

    #[test]
    fn the_game_join_load_time_line_carries_the_account() {
        let l = line(
            "[FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:1234567, join_time:0.71, universeid:7654321, referral_page:RequestGame, sid:00000000-1111-2222-3333-444444444444, clienttime:1791000000.5, userid:11111111, ",
        );
        assert_eq!(game_join_user_id(&l), Some(11111111));
        assert_eq!(game_join_place_id(&l), Some(1234567));
    }

    #[test]
    fn userid_outside_the_game_join_line_does_not_count() {
        assert_eq!(game_join_user_id(&line("[FLog::Output] something userid:5, ")), None);
        assert_eq!(
            game_join_user_id(&line("[FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:1, userid:, ")),
            None
        );
        assert_eq!(
            game_join_user_id(&line("[FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:1, userid:0, ")),
            None
        );
    }
}
