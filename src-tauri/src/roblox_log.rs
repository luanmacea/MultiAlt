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
