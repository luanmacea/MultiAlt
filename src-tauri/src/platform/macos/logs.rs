pub fn candidate_log_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = home_dir() {
        dirs.push(home.join("Library").join("Logs").join("Roblox"));
        dirs.push(home.join("Library").join("Roblox").join("logs"));
    }
    dirs
}

fn all_log_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in candidate_log_dirs() {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("log") {
                        out.push(path);
                    }
                }
            }
        }
    }
    out
}

pub fn latest_log_file_for_pid(pid: u32) -> Option<PathBuf> {
    let pid_str = pid.to_string();
    let files = all_log_files();

    let mut pid_best: Option<(SystemTime, PathBuf)> = None;
    let mut any_best: Option<(SystemTime, PathBuf)> = None;

    for path in files {
        let modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH);
        if any_best
            .as_ref()
            .map(|(ts, _)| modified > *ts)
            .unwrap_or(true)
        {
            any_best = Some((modified, path.clone()));
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if name.contains(&pid_str)
            && pid_best
                .as_ref()
                .map(|(ts, _)| modified > *ts)
                .unwrap_or(true)
        {
            pid_best = Some((modified, path));
        }
    }

    pid_best.or(any_best).map(|(_, p)| p)
}

pub fn read_log_delta(path: &Path, cursor: &mut u64) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("Open log failed: {}", e))?;
    let len = file
        .metadata()
        .map_err(|e| format!("Stat log failed: {}", e))?
        .len();
    if *cursor > len {
        *cursor = 0;
    }
    file.seek(SeekFrom::Start(*cursor))
        .map_err(|e| format!("Seek log failed: {}", e))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .map_err(|e| format!("Read log failed: {}", e))?;
    *cursor = len;
    Ok(buf)
}
