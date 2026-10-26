#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Account {
    #[serde(default)]
    pub valid: bool,
    #[serde(default, deserialize_with = "string_or_default")]
    pub security_token: String,
    #[serde(default, deserialize_with = "string_or_default")]
    pub username: String,
    #[serde(
        default = "default_timestamp",
        deserialize_with = "csharp_datetime::deserialize_or_default",
        serialize_with = "csharp_datetime::serialize"
    )]
    pub last_use: DateTime<Utc>,
    #[serde(rename = "Alias", default, deserialize_with = "string_or_default")]
    pub alias: String,
    #[serde(
        rename = "Description",
        default,
        deserialize_with = "string_or_default"
    )]
    pub description: String,
    #[serde(rename = "Password", default, deserialize_with = "string_or_default")]
    pub password: String,
    /// As senhas guardadas antes da atual, da mais nova para a mais velha (no
    /// máximo `PASSWORD_HISTORY_MAX`). Rede contra perda: uma senha errada
    /// gravada por cima da certa ainda deixa a certa aqui. Só se escreve por
    /// `replace_password`. Ver CLAUDE.md, "Senha guardada nunca se perde".
    #[serde(rename = "PasswordHistory", default, skip_serializing_if = "Vec::is_empty")]
    pub password_history: Vec<String>,
    #[serde(
        default = "default_group",
        deserialize_with = "group_or_default",
        skip_serializing_if = "is_default_group"
    )]
    pub group: String,
    #[serde(rename = "UserID", default)]
    pub user_id: i64,
    #[serde(default, deserialize_with = "fields_or_default")]
    pub fields: HashMap<String, String>,
    #[serde(
        default = "default_timestamp",
        deserialize_with = "csharp_datetime::deserialize_or_default",
        serialize_with = "csharp_datetime::serialize"
    )]
    pub last_attempted_refresh: DateTime<Utc>,
    #[serde(
        rename = "BrowserTrackerID",
        alias = "BrowserTrackerId",
        default,
        deserialize_with = "string_or_default"
    )]
    pub browser_tracker_id: String,
    /// Quando o Roblox pediu a verificação "confirme que é humano" e ela não foi
    /// resolvida (RFC 3339, com fuso). Some quando a pessoa resolve ou a conta
    /// entra num jogo (`human_check_mark`). Sem marca, o campo nem é gravado.
    #[serde(rename = "HumanCheckAt", default, skip_serializing_if = "Option::is_none")]
    pub human_check_at: Option<String>,
}

fn default_group() -> String {
    "Default".to_string()
}

fn default_timestamp() -> DateTime<Utc> {
    Utc::now()
}

fn is_default_group(group: &String) -> bool {
    group == "Default"
}

fn string_or_default<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn group_or_default<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_else(default_group))
}

fn fields_or_default<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(
        Option::<HashMap<String, Option<String>>>::deserialize(deserializer)?
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| (key, value.unwrap_or_default()))
            .collect(),
    )
}

mod csharp_datetime {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.f";

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = date.format(FORMAT).to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_datetime(&s)
    }

    pub fn deserialize_or_default<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let Some(s) = Option::<String>::deserialize(deserializer)? else {
            return Ok(Utc::now());
        };
        if s.trim().is_empty() {
            return Ok(Utc::now());
        }
        parse_datetime(&s)
    }

    fn parse_datetime<E>(s: &str) -> Result<DateTime<Utc>, E>
    where
        E: serde::de::Error,
    {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
            return Ok(dt.with_timezone(&Utc));
        }

        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, FORMAT) {
            return Ok(Utc.from_utc_datetime(&dt));
        }

        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S") {
            return Ok(Utc.from_utc_datetime(&dt));
        }

        Err(serde::de::Error::custom(format!(
            "Failed to parse datetime: {}",
            s
        )))
    }
}

impl Default for Account {
    fn default() -> Self {
        Self {
            valid: false,
            security_token: String::new(),
            username: String::new(),
            last_use: Utc::now(),
            alias: String::new(),
            description: String::new(),
            password: String::new(),
            password_history: Vec::new(),
            group: default_group(),
            user_id: 0,
            fields: HashMap::new(),
            last_attempted_refresh: Utc::now(),
            browser_tracker_id: String::new(),
            human_check_at: None,
        }
    }
}

/// Quantas senhas antigas a conta guarda.
pub const PASSWORD_HISTORY_MAX: usize = 5;

impl Account {
    /// O único jeito de trocar a senha **guardada** de uma conta que já existe:
    /// a anterior vai para o histórico, e senha vazia nunca apaga a guardada.
    pub fn replace_password(&mut self, new_password: &str) {
        if new_password.is_empty() || new_password == self.password {
            return;
        }
        let previous = std::mem::take(&mut self.password);
        if !previous.is_empty() {
            self.password_history.retain(|p| *p != previous && p != new_password);
            self.password_history.insert(0, previous);
            self.password_history.truncate(PASSWORD_HISTORY_MAX);
        } else {
            self.password_history.retain(|p| p != new_password);
        }
        self.password = new_password.to_string();
    }

    /// O cookie, a senha e o histórico de senhas vêm sempre do que está
    /// guardado, nunca de uma cópia da tela (que pode estar velha ou sem o
    /// histórico): editar apelido ou grupo não pode mexer em credencial.
    pub fn keep_credentials_from(&mut self, stored: &Account) {
        self.security_token = stored.security_token.clone();
        self.password = stored.password.clone();
        self.password_history = stored.password_history.clone();
    }

    pub fn new(security_token: String, username: String, user_id: i64) -> Self {
        Self {
            valid: true,
            security_token,
            username,
            user_id,
            last_use: Utc::now(),
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn get_field(&self, name: &str) -> Option<&String> {
        self.fields.get(name)
    }

    pub fn set_field(&mut self, name: String, value: String) {
        self.fields.insert(name, value);
    }

    pub fn remove_field(&mut self, name: &str) {
        self.fields.remove(name);
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;

    // ── senha guardada nunca se perde (25/10/2026) ──────────────────────────
    //
    // Duas perdas de senha reais: a troca de senha de uma conta só não gravava
    // a nova (até a v2.1.0), e o criador de contas gravava a senha de outra
    // identidade por cima da conta anterior (até a v2.3.0). Toda troca da
    // senha guardada passa por `replace_password`, que guarda a anterior.

    #[test]
    fn replacing_a_password_keeps_the_previous_one_in_the_history() {
        let mut a = Account::new("c".into(), "ann".into(), 1);
        a.replace_password("first");
        a.replace_password("second");
        a.replace_password("third");
        assert_eq!(a.password, "third");
        assert_eq!(a.password_history, vec!["second".to_string(), "first".to_string()]);
    }

    /// A tela manda a conta sem o histórico (ou com cookie velho): o que vale é
    /// o guardado.
    #[test]
    fn editing_from_the_screen_keeps_the_stored_credentials_and_history() {
        let mut stored = Account::new("stored-cookie".into(), "ann".into(), 1);
        stored.replace_password("old");
        stored.replace_password("current");
        let mut from_screen = Account::new("stale-cookie".into(), "ann".into(), 1);
        from_screen.alias = "Main".into();
        from_screen.keep_credentials_from(&stored);
        assert_eq!(from_screen.security_token, "stored-cookie");
        assert_eq!(from_screen.password, "current");
        assert_eq!(from_screen.password_history, vec!["old".to_string()]);
        assert_eq!(from_screen.alias, "Main");
    }

    #[test]
    fn an_empty_password_never_erases_the_saved_one() {
        let mut a = Account::new("c".into(), "ann".into(), 1);
        a.replace_password("kept");
        a.replace_password("");
        assert_eq!(a.password, "kept");
        assert!(a.password_history.is_empty());
    }

    #[test]
    fn the_same_password_again_changes_nothing_and_the_history_has_no_repeats() {
        let mut a = Account::new("c".into(), "ann".into(), 1);
        a.replace_password("one");
        a.replace_password("one");
        assert!(a.password_history.is_empty());
        a.replace_password("two");
        a.replace_password("one");
        a.replace_password("two");
        assert_eq!(a.password, "two");
        assert_eq!(a.password_history, vec!["one".to_string()]);
    }

    #[test]
    fn the_history_keeps_only_the_latest_passwords() {
        let mut a = Account::new("c".into(), "ann".into(), 1);
        for i in 0..20 {
            a.replace_password(&format!("p{i}"));
        }
        assert_eq!(a.password_history.len(), PASSWORD_HISTORY_MAX);
        assert_eq!(a.password_history[0], "p18");
    }

    #[test]
    fn the_history_survives_a_save_and_old_files_without_it_still_load() {
        let mut a = Account::new("c".into(), "ann".into(), 1);
        a.replace_password("old");
        a.replace_password("new");
        let json = serde_json::to_string(&a).unwrap();
        let back = parse(&json);
        assert_eq!(back.password_history, vec!["old".to_string()]);
        let legacy = parse(r#"{"Username":"ann","UserID":1,"Password":"x"}"#);
        assert!(legacy.password_history.is_empty());
        assert!(!serde_json::to_string(&legacy).unwrap().contains("PasswordHistory"), "vazio não vai para o arquivo");
    }

    /// Trava: toda escrita da senha guardada fora do construtor de conta nova
    /// passa por `replace_password`. Escrever `.password =` direto num ponto
    /// novo do código falha aqui — leia a regra "Senha guardada nunca se perde"
    /// do CLAUDE.md antes de mexer nesta lista.
    #[test]
    fn every_write_of_a_saved_password_goes_through_replace_password() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found: Vec<String> = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                // Só o código de produção: os testes montam contas à vontade.
                let production = text.split("#[cfg(test)]").next().unwrap_or("");
                for line in production.lines() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("//") {
                        continue;
                    }
                    if trimmed.contains(".password = ") {
                        let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                        found.push(format!("{rel}: {}", trimmed.trim()));
                    }
                }
            }
        }
        found.sort();
        // Contas **novas** (nenhuma senha guardada antes para perder) e o
        // próprio `replace_password`.
        let allowed = [
            "chromium/commands.rs: account.password = password;",
            "chromium/signup_session.rs: account.password = password_to_save(typed_password, &identity.password);",
            "commands/generators.rs: model.password = account.password.clone();",
            "data/accounts/commands.rs: account.password = password;",
            "data/accounts/model.rs: self.password = new_password.to_string();",
            // Cópia do que já está guardado (editar pela tela não mexe em credencial).
            "data/accounts/model.rs: self.password = stored.password.clone();",
        ];
        let unexpected: Vec<&String> = found.iter().filter(|f| !allowed.contains(&f.as_str())).collect();
        assert!(
            unexpected.is_empty(),
            "escrita nova da senha guardada fora de `replace_password`:\n{unexpected:#?}"
        );
    }

    fn parse(json: &str) -> Account {
        serde_json::from_str::<Account>(json).expect("account should deserialize")
    }

    #[test]
    fn null_string_fields_become_empty_strings() {
        let account = parse(
            r#"{
                "SecurityToken": null,
                "Username": null,
                "Alias": null,
                "Description": null,
                "Password": null,
                "BrowserTrackerID": null,
                "UserID": 7
            }"#,
        );
        assert_eq!(account.security_token, "");
        assert_eq!(account.username, "");
        assert_eq!(account.alias, "");
        assert_eq!(account.description, "");
        assert_eq!(account.password, "");
        assert_eq!(account.browser_tracker_id, "");
        assert_eq!(account.user_id, 7);
    }

    #[test]
    fn missing_string_fields_become_empty_strings() {
        let account = parse(r#"{"UserID": 1}"#);
        assert_eq!(account.username, "");
        assert_eq!(account.security_token, "");
        assert!(!account.valid);
        assert!(account.fields.is_empty());
    }

    #[test]
    fn browser_tracker_id_accepts_the_legacy_camel_case_alias() {
        let account = parse(r#"{"BrowserTrackerId": "123456789012"}"#);
        assert_eq!(account.browser_tracker_id, "123456789012");
    }

    #[test]
    fn missing_or_null_group_falls_back_to_default() {
        assert_eq!(parse(r#"{"UserID": 1}"#).group, "Default");
        assert_eq!(parse(r#"{"Group": null}"#).group, "Default");
        assert_eq!(parse(r#"{"Group": "moderadas"}"#).group, "moderadas");
    }

    #[test]
    fn the_default_group_is_not_serialized() {
        let default_account = Account::default();
        assert_eq!(default_account.group, "Default");
        let json = serde_json::to_value(&default_account).expect("serialize");
        assert!(json.get("Group").is_none());

        let mut grouped = Account::default();
        grouped.group = "moderadas".to_string();
        let json = serde_json::to_value(&grouped).expect("serialize");
        assert_eq!(json.get("Group").and_then(|v| v.as_str()), Some("moderadas"));
    }

    #[test]
    fn legacy_csharp_datetimes_are_parsed() {
        let account = parse(
            r#"{"LastUse": "2020-01-02T03:04:05.1234567", "LastAttemptedRefresh": "2021-06-07T08:09:10"}"#,
        );
        assert_eq!(account.last_use.format("%Y-%m-%dT%H:%M:%S").to_string(), "2020-01-02T03:04:05");
        assert_eq!(
            account.last_attempted_refresh.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2021-06-07T08:09:10"
        );
    }

    #[test]
    fn rfc3339_datetimes_are_parsed_and_converted_to_utc() {
        let account = parse(r#"{"LastUse": "2020-01-02T05:04:05+02:00"}"#);
        assert_eq!(
            account.last_use.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2020-01-02T03:04:05"
        );
    }

    #[test]
    fn null_or_blank_datetimes_fall_back_to_now_instead_of_failing() {
        let before = Utc::now();
        let account = parse(r#"{"LastUse": null, "LastAttemptedRefresh": "   "}"#);
        let after = Utc::now();
        assert!(account.last_use >= before && account.last_use <= after);
        assert!(account.last_attempted_refresh >= before && account.last_attempted_refresh <= after);
    }

    #[test]
    fn an_unparseable_datetime_is_an_error() {
        assert!(serde_json::from_str::<Account>(r#"{"LastUse": "not-a-date"}"#).is_err());
    }

    #[test]
    fn datetimes_serialize_in_the_csharp_format() {
        let account = parse(r#"{"LastUse": "2020-01-02T03:04:05"}"#);
        let json = serde_json::to_value(&account).expect("serialize");
        assert_eq!(
            json.get("LastUse").and_then(|v| v.as_str()),
            Some("2020-01-02T03:04:05")
        );
    }

    #[test]
    fn fields_or_default_handles_a_null_map() {
        let account = parse(r#"{"Fields": null}"#);
        assert!(account.fields.is_empty());
    }

    #[test]
    fn fields_or_default_turns_null_values_into_empty_strings() {
        let account = parse(r#"{"Fields": {"Window_Width": null, "Note": "hi"}}"#);
        assert_eq!(account.fields.get("Window_Width").map(String::as_str), Some(""));
        assert_eq!(account.fields.get("Note").map(String::as_str), Some("hi"));
    }

    #[test]
    fn field_helpers_set_get_and_remove() {
        let mut account = Account::new("token".into(), "user".into(), 42);
        assert!(account.valid);
        assert_eq!(account.user_id, 42);
        assert_eq!(account.get_field("Note"), None);

        account.set_field("Note".into(), "hello".into());
        assert_eq!(account.get_field("Note").map(String::as_str), Some("hello"));

        account.set_field("Note".into(), "bye".into());
        assert_eq!(account.get_field("Note").map(String::as_str), Some("bye"));

        account.remove_field("Note");
        assert_eq!(account.get_field("Note"), None);
    }
}
