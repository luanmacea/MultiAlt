#[derive(Debug, Deserialize)]
struct AccountQuery {
    #[serde(alias = "account", alias = "Account")]
    account: Option<String>,
    #[serde(alias = "password", alias = "Password")]
    password: Option<String>,
    #[serde(alias = "placeId", alias = "PlaceId", alias = "placeid")]
    place_id: Option<String>,
    #[serde(alias = "jobId", alias = "JobId", alias = "jobid")]
    job_id: Option<String>,
    #[serde(alias = "userId", alias = "UserId", alias = "userid")]
    user_id: Option<String>,
    #[serde(alias = "field", alias = "Field")]
    field: Option<String>,
    #[serde(alias = "value", alias = "Value")]
    value: Option<String>,
    #[serde(alias = "cookie", alias = "Cookie")]
    cookie: Option<String>,
    #[serde(alias = "group", alias = "Group")]
    group: Option<String>,
    #[serde(alias = "username", alias = "Username")]
    username: Option<String>,
    #[serde(alias = "followUser", alias = "FollowUser")]
    follow_user: Option<String>,
    #[serde(alias = "joinVip", alias = "JoinVIP")]
    join_vip: Option<String>,
    #[serde(alias = "includeCookies", alias = "IncludeCookies")]
    include_cookies: Option<String>,
}
