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
            group: default_group(),
            user_id: 0,
            fields: HashMap::new(),
            last_attempted_refresh: Utc::now(),
            browser_tracker_id: String::new(),
        }
    }
}

impl Account {
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
