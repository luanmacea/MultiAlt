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
