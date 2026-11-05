pub fn build_launch_url(
    ticket: &str,
    place_id: i64,
    job_id: &str,
    browser_tracker_id: &str,
    launch_data: &str,
    follow_user: bool,
    join_vip: bool,
    access_code: &str,
    link_code: &str,
    is_teleport: bool,
) -> String {
    let launch_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let ld_param = if launch_data.is_empty() {
        String::new()
    } else {
        format!("&launchData={}", urlencoding::encode(launch_data))
    };

    let place_launcher_url = if join_vip {
        let access_param = if access_code.is_empty() {
            String::new()
        } else {
            format!("&accessCode={}", urlencoding::encode(access_code))
        };
        let link_param = if link_code.is_empty() {
            String::new()
        } else {
            format!("&linkCode={}", urlencoding::encode(link_code))
        };
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestPrivateGame&placeId={}{}{}{}",
            place_id, access_param, link_param, ld_param
        )
    } else if follow_user {
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestFollowUser&userId={}{}",
            place_id, ld_param
        )
    } else {
        let req = if job_id.is_empty() {
            "RequestGame"
        } else {
            "RequestGameJob"
        };
        let gid = if job_id.is_empty() {
            String::new()
        } else {
            format!("&gameId={}", job_id)
        };
        let tp = if is_teleport { "&isTeleport=true" } else { "" };
        format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request={}&browserTrackerId={}&placeId={}{}&isPlayTogetherGame=false{}{}",
            req, browser_tracker_id, place_id, gid, tp, ld_param
        )
    };

    format!(
        "roblox-player:1+launchmode:play+gameinfo:{}+launchtime:{}+placelauncherurl:{}+browsertrackerid:{}+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp",
        ticket,
        launch_time,
        urlencoding::encode(&place_launcher_url),
        browser_tracker_id
    )
}

pub fn launch_url(url: &str) -> Result<(), String> {
    pre_launch_multi_step();

    // `open <link>` sozinho entrega o link ao Roblox já aberto; com o Multi
    // Roblox, `open -n -a <app>` abre outra instância (ver platform/mac_open.rs).
    let app = get_roblox_path().ok();
    let plan = crate::platform::mac_open::open_plan(
        url,
        MULTI_ROBLOX_ENABLED.load(Ordering::Relaxed),
        app.as_deref(),
    );
    if let Some(warning) = plan.warning {
        eprintln!("[launch] {}", warning);
    }
    match Command::new("open").args(&plan.args).status() {
        Ok(status) if status.success() => Ok(()),
        // `-n -a` falhou (app movido, permissão): tenta o link puro antes de
        // desistir — regra nº 1, abrir do jeito que der.
        _ if plan.args.len() > 1 => match Command::new("open").arg(url).status() {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!("open exited with {} for the Roblox link", status)),
            Err(e) => Err(format!("Failed to run open: {}", e)),
        },
        Ok(status) => Err(format!("open exited with {} for the Roblox link", status)),
        Err(e) => Err(format!("Failed to run open: {}", e)),
    }
}

pub fn launch_old_join(
    ticket: &str,
    place_id: i64,
    job_id: &str,
    launch_data: &str,
    follow_user: bool,
    join_vip: bool,
    access_code: &str,
    link_code: &str,
    is_teleport: bool,
) -> Result<(), String> {
    let browser_tracker_id = generate_browser_tracker_id();
    let url = build_launch_url(
        ticket,
        place_id,
        job_id,
        &browser_tracker_id,
        launch_data,
        follow_user,
        join_vip,
        access_code,
        link_code,
        is_teleport,
    );
    launch_url(&url)
}

