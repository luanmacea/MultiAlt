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

/// `AccountQuery` is the single query shape every endpoint takes. The Lua
/// clients and the old C# manager spell the parameters differently
/// (`PlaceId`, `placeId`, `placeid`), so the aliases are part of the contract.
#[cfg(test)]
mod server_query_tests {
    use super::*;
    use axum::extract::Query;
    use axum::http::Uri;

    fn parse(query: &str) -> AccountQuery {
        let uri: Uri = format!("http://127.0.0.1/Endpoint?{}", query)
            .parse()
            .unwrap();
        Query::<AccountQuery>::try_from_uri(&uri)
            .expect("query should deserialize")
            .0
    }

    #[test]
    fn every_parameter_is_optional() {
        let q = parse("");
        assert!(q.account.is_none());
        assert!(q.password.is_none());
        assert!(q.place_id.is_none());
        assert!(q.job_id.is_none());
        assert!(q.user_id.is_none());
        assert!(q.field.is_none());
        assert!(q.value.is_none());
        assert!(q.cookie.is_none());
        assert!(q.group.is_none());
        assert!(q.username.is_none());
        assert!(q.follow_user.is_none());
        assert!(q.join_vip.is_none());
        assert!(q.include_cookies.is_none());
    }

    #[test]
    fn the_account_parameter_takes_both_casings() {
        assert_eq!(parse("Account=alt_one").account.as_deref(), Some("alt_one"));
        assert_eq!(parse("account=alt_one").account.as_deref(), Some("alt_one"));
    }

    #[test]
    fn the_place_id_takes_all_three_spellings() {
        for query in ["PlaceId=1234", "placeId=1234", "placeid=1234", "place_id=1234"] {
            assert_eq!(parse(query).place_id.as_deref(), Some("1234"), "{}", query);
        }
    }

    #[test]
    fn the_job_id_takes_all_three_spellings() {
        for query in ["JobId=job-1", "jobId=job-1", "jobid=job-1", "job_id=job-1"] {
            assert_eq!(parse(query).job_id.as_deref(), Some("job-1"), "{}", query);
        }
    }

    #[test]
    fn the_user_id_takes_all_three_spellings() {
        for query in ["UserId=99", "userId=99", "userid=99", "user_id=99"] {
            assert_eq!(parse(query).user_id.as_deref(), Some("99"), "{}", query);
        }
    }

    #[test]
    fn include_cookies_takes_both_casings() {
        assert_eq!(
            parse("IncludeCookies=true").include_cookies.as_deref(),
            Some("true")
        );
        assert_eq!(
            parse("includeCookies=TRUE").include_cookies.as_deref(),
            Some("TRUE")
        );
    }

    /// `JoinVIP` is spelled in full caps by the Lua client.
    #[test]
    fn the_launch_flags_take_both_casings() {
        assert_eq!(parse("JoinVIP=true").join_vip.as_deref(), Some("true"));
        assert_eq!(parse("joinVip=true").join_vip.as_deref(), Some("true"));
        assert_eq!(parse("FollowUser=true").follow_user.as_deref(), Some("true"));
        assert_eq!(parse("followUser=true").follow_user.as_deref(), Some("true"));
    }

    #[test]
    fn the_remaining_parameters_take_both_casings() {
        assert_eq!(parse("Password=secret").password.as_deref(), Some("secret"));
        assert_eq!(parse("password=secret").password.as_deref(), Some("secret"));
        assert_eq!(parse("Field=Note").field.as_deref(), Some("Note"));
        assert_eq!(parse("field=Note").field.as_deref(), Some("Note"));
        assert_eq!(parse("Value=hi").value.as_deref(), Some("hi"));
        assert_eq!(parse("value=hi").value.as_deref(), Some("hi"));
        assert_eq!(parse("Cookie=tok").cookie.as_deref(), Some("tok"));
        assert_eq!(parse("cookie=tok").cookie.as_deref(), Some("tok"));
        assert_eq!(parse("Group=Bots").group.as_deref(), Some("Bots"));
        assert_eq!(parse("group=Bots").group.as_deref(), Some("Bots"));
        assert_eq!(parse("Username=target").username.as_deref(), Some("target"));
        assert_eq!(parse("username=target").username.as_deref(), Some("target"));
    }

    /// Values arrive percent-encoded and are decoded by the extractor.
    #[test]
    fn values_are_url_decoded() {
        assert_eq!(
            parse("Value=hello%20world&Field=My%2BField").value.as_deref(),
            Some("hello world")
        );
        assert_eq!(parse("Password=a%26b").password.as_deref(), Some("a&b"));
        // `+` is the form encoding of a space.
        assert_eq!(parse("Value=a+b").value.as_deref(), Some("a b"));
    }

    /// Unknown parameters are ignored instead of failing the request, and an
    /// empty value stays an empty string (the handlers reject those).
    #[test]
    fn unknown_parameters_are_ignored_and_empty_values_survive() {
        let q = parse("Account=alt_one&somethingElse=1");
        assert_eq!(q.account.as_deref(), Some("alt_one"));

        assert_eq!(parse("Account=").account.as_deref(), Some(""));
    }
}
