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
// ... [FLog::Network] UDMUX Address = 128.116.x.33, Port = 54148 | RCC Server Address = 10.x.x.x, Port = 54148
// ... [FLog::Output] Server Prefix: 0.743.0.7431129_20261014T203500Z_RCC_b25e4
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

/// O endereço do proxy UDMUX do servidor em que o cliente entrou:
/// `UDMUX Address = 128.116.21.33, Port = ...`. É o IP **público** do
/// datacenter (o `RCC Server Address`, e o `at 10.x` do `Joining game`, são
/// internos). Só devolve um IP que se lê como IP — quem decide se ele é
/// público é quem vai geolocalizá-lo.
pub fn udmux_address(line: &str) -> Option<String> {
    const MARKER: &str = "UDMUX Address = ";
    let start = line.find(MARKER)? + MARKER.len();
    let rest = &line[start..];
    let end = rest
        .find(|c: char| c == ',' || c == '|' || c.is_whitespace())
        .unwrap_or(rest.len());
    let address = rest[..end].trim();
    address
        .parse::<std::net::IpAddr>()
        .ok()
        .map(|_| address.to_string())
}

/// Quando o servidor subiu, em ms Unix, pela linha
/// `Server Prefix: 0.743.0.7431129_20261014T203500Z_RCC_b25e4` — o carimbo
/// `AAAAMMDDTHHMMSSZ` (UTC) é o início do processo do servidor. Linha fora do
/// formato devolve `None` (nunca entra em pânico).
pub fn server_started_at_ms(line: &str) -> Option<i64> {
    const MARKER: &str = "Server Prefix: ";
    let start = line.find(MARKER)? + MARKER.len();
    let prefix = line[start..].split_whitespace().next()?;
    prefix.split('_').find_map(|part| {
        // `20261014T203500Z`: 8 dígitos, T, 6 dígitos, Z.
        let bytes = part.as_bytes();
        let shaped = bytes.len() == 16
            && bytes[8] == b'T'
            && bytes[15] == b'Z'
            && bytes[..8].iter().all(u8::is_ascii_digit)
            && bytes[9..15].iter().all(u8::is_ascii_digit);
        if !shaped {
            return None;
        }
        chrono::NaiveDateTime::parse_from_str(part, "%Y%m%dT%H%M%SZ")
            .ok()
            .map(|dt| dt.and_utc().timestamp_millis())
    })
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

    #[test]
    fn the_udmux_line_gives_the_public_address() {
        let l = line(
            "[FLog::Network] UDMUX Address = 128.116.21.33, Port = 54148 | RCC Server Address = 10.0.0.1, Port = 54148",
        );
        assert_eq!(udmux_address(&l).as_deref(), Some("128.116.21.33"));
    }

    #[test]
    fn a_malformed_udmux_line_gives_nothing() {
        for body in [
            "[FLog::Network] UDMUX Address = , Port = 54148",
            "[FLog::Network] UDMUX Address = not-an-ip, Port = 1",
            "[FLog::Network] UDMUX Address = 128.116.21",
            "[FLog::Network] RCC Server Address = 10.0.0.1, Port = 54148",
            "[FLog::Network] UDMUX Address =",
        ] {
            assert_eq!(udmux_address(&line(body)), None, "{body}");
        }
        // Sem nada depois do IP (linha cortada) ainda serve.
        assert_eq!(
            udmux_address("UDMUX Address = 128.116.30.33").as_deref(),
            Some("128.116.30.33")
        );
    }

    #[test]
    fn the_server_prefix_gives_when_the_server_started() {
        let l = line("[FLog::Output] Server Prefix: 0.743.0.7431129_20261014T235657Z_RCC_79c67");
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 10, 14)
            .unwrap()
            .and_hms_opt(23, 56, 57)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        assert_eq!(server_started_at_ms(&l), Some(expected));
    }

    #[test]
    fn a_malformed_server_prefix_gives_nothing() {
        for body in [
            "[FLog::Output] Server Prefix: ",
            "[FLog::Output] Server Prefix: 0.743.0.7431129_RCC_79c67",
            "[FLog::Output] Server Prefix: 0.743.0.7431129_20261399T999999Z_RCC_79c67",
            "[FLog::Output] Server Prefix: 0.743.0.7431129_2026101XT235657Z_RCC_79c67",
            "[FLog::Output] Server Prefix: 0.743.0.7431129_20261014T235657_RCC_79c67",
            "[FLog::Output] Server Prefix: ééééééééééééééééé",
            "[FLog::Output] Server Prefix:",
            "[FLog::Output] Joining game",
        ] {
            assert_eq!(server_started_at_ms(&line(body)), None, "{body}");
        }
    }
}
