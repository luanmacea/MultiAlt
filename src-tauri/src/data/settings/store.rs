pub struct SettingsStore {
    ini: Mutex<IniFile>,
    file_path: PathBuf,
    /// Ligado quando o INI existe mas não pôde ser lido. `IniFile::load` engole
    /// o erro de I/O, então sem este latch `apply_defaults` gravaria um arquivo
    /// só com os defaults por cima das settings do usuário — mesmo latch do
    /// `ScriptStore`.
    load_failed: std::sync::atomic::AtomicBool,
}

impl SettingsStore {
    pub fn new(file_path: PathBuf) -> Self {
        let file_existed = file_path.exists();
        let (ini, failed) = if file_existed {
            match fs::read_to_string(&file_path) {
                Ok(raw) => {
                    let mut ini = IniFile::new();
                    ini.parse(&raw);
                    (ini, false)
                }
                Err(_) => (IniFile::new(), true),
            }
        } else {
            (IniFile::new(), false)
        };

        let store = Self {
            ini: Mutex::new(ini),
            file_path,
            load_failed: std::sync::atomic::AtomicBool::new(failed),
        };

        store.apply_defaults(file_existed);
        store
    }

    fn apply_defaults(&self, settings_file_existed: bool) {
        let mut ini = self.ini.lock().unwrap();

        let defaults: &[(&str, &str, Option<&str>)] = &[
            ("CheckForUpdates", "true", None),
            ("UpdaterReleaseChannel", "beta", None),
            // Default = a edição que está rodando: quem instalou a completa
            // continua recebendo a completa. Valor já gravado nunca é trocado.
            ("UpdaterFeatureChannel", crate::RUNNING_FEATURE_CHANNEL, None),
            ("AccountJoinDelay", "8", None),
            ("AsyncJoin", "false", None),
            (
                "WaitForGameJoin",
                "true",
                Some("Multi-launch: start the next account as soon as the previous one is in the game (never before AccountJoinDelay's 8 s floor, at most 20 s)"),
            ),
            (
                "WrongAccountQueue",
                "ask",
                Some("Multi-launch: when a client opens as a different Roblox account, ask whether to keep launching (ask), keep launching (continue) or stop the queue (stop)"),
            ),
            (
                "EnableMultiRbx",
                "true",
                Some("Launch: keep several Roblox clients open at once, wherever they are opened from, while this app is open"),
            ),
            ("DisableAgingAlert", "false", None),
            (
                "JoinFriendsByFollowing",
                "true",
                Some("Friends/Follow tabs: join by following the player (RequestFollowUser), like the old RAM, instead of asking for the exact server; helps avoid CAPTCHA"),
            ),
            ("HideUsernames", "false", None),
            ("WrapLongNames", "false", None),
            (
                "ServerRegionFormat",
                "<city>, <countryCode>",
                // O comentario apontava ip-api.com, que nao tem nada a ver com os
                // tokens que `format_region` substitui de fato.
                Some("Tokens: <city>, <region>, <country>, <countryCode>, <ip>; other text is kept as typed"),
            ),
            ("MaxRecentGames", "8", None),
            ("MaxRecentJobs", "12", None),
            (
                "GroupOrder",
                "[]",
                // JSON, e nao lista por virgula como as outras chaves de lista:
                // nome de grupo e texto livre do usuario e pode conter virgula.
                Some("Manual order of the account groups, as a JSON array of group names"),
            ),
            ("Language", "en", None),
            ("AutoCookieRefresh", "true", None),
            ("AutoCloseLastProcess", "false", None),
            ("AutoCloseRobloxForMultiRbx", "false", None),
            ("ShowPresence", "true", None),
            (
                "ShowAccountNameOnWindow",
                "true",
                Some("Titles each Roblox window this app tracks \"<account> — Roblox\", name first (masked when names are hidden)"),
            ),
            ("PresenceUpdateRate", "5", None),
            ("WarnOnOnlineJoin", "true", None),
            ("WarnOnCopyCredential", "true", None),
            (
                "CheckModerationBeforeLaunch",
                "true",
                Some("Check if an account is banned right before launching it, and skip it if so"),
            ),
            ("UnlockFPS", "false", None),
            ("MaxFPSValue", "120", None),
            ("CustomClientSettings", "", None),
            ("OverrideClientVolume", "false", None),
            ("ClientVolume", "0.5", None),
            ("OverrideClientGraphics", "false", None),
            ("ClientGraphicsLevel", "10", None),
            ("OverrideClientWindowSize", "false", None),
            ("ClientWindowWidth", "1280", None),
            ("ClientWindowHeight", "720", None),
            ("StartRobloxMinimized", "false", None),
            (
                "AutoArrangeGrid",
                "true",
                Some("New Roblox windows take the first free cell of the window grid (GridMonitors, GridGap)"),
            ),
            (
                "GridAllowSmallWindows",
                "false",
                Some("Grid cells may be smaller than Roblox's minimum window (only clients MultiAlt launched)"),
            ),
            (
                "GridAutoFit",
                "false",
                Some("Grid cells take the largest landscape size that fits every window on the grid monitors (replaces the window size and GridAllowSmallWindows)"),
            ),
            (
                "GridBorderless",
                "false",
                Some("Grid windows lose their title bar and border; turning it off gives them back"),
            ),
            ("StartOnPCStartup", "false", None),
            ("MinimizeToTray", "false", None),
            ("ThemeWindowsNavbar", "true", None),
            ("RestrictedBackgroundStyle", "warp", None),
            ("BottingEnabled", "false", None),
            (
                "KeepPcAwake",
                "true",
                Some("Keep the PC from sleeping (the screen may still turn off) while AFK Mode, Auto Rejoin or auto-reconnect runs"),
            ),
            (
                "AutoReconnect",
                "false",
                Some("Default for every account: reopen a client this app opened in the same game when it drops (each account's AutoReconnect field wins)"),
            ),
            (
                "ReconnectSameServer",
                "true",
                Some("Auto-reconnect rejoins the exact server the account was in; off = rejoin the game and let Roblox pick the server (for games that move you to a reserved server)"),
            ),
            ("BottingUseSharedClientProfile", "true", None),
            ("BottingAutoShareLaunchFields", "true", None),
            ("BottingDualPanelDialog", "true", None),
            ("BottingPlayerUnlockFPS", "false", None),
            ("BottingPlayerMaxFPSValue", "120", None),
            ("BottingPlayerCustomClientSettings", "", None),
            ("BottingPlayerOverrideClientVolume", "false", None),
            ("BottingPlayerClientVolume", "0.5", None),
            ("BottingPlayerOverrideClientGraphics", "false", None),
            ("BottingPlayerClientGraphicsLevel", "10", None),
            ("BottingPlayerOverrideClientWindowSize", "false", None),
            ("BottingPlayerClientWindowWidth", "1280", None),
            ("BottingPlayerClientWindowHeight", "720", None),
            ("BottingPlayerStartRobloxMinimized", "false", None),
            ("BottingBotUnlockFPS", "false", None),
            ("BottingBotMaxFPSValue", "120", None),
            ("BottingBotCustomClientSettings", "", None),
            ("BottingBotOverrideClientVolume", "false", None),
            ("BottingBotClientVolume", "0.5", None),
            ("BottingBotOverrideClientGraphics", "false", None),
            ("BottingBotClientGraphicsLevel", "10", None),
            ("BottingBotOverrideClientWindowSize", "false", None),
            ("BottingBotClientWindowWidth", "1280", None),
            ("BottingBotClientWindowHeight", "720", None),
            ("BottingBotStartRobloxMinimized", "false", None),
            ("BottingDefaultIntervalMinutes", "19", None),
            ("BottingLaunchDelaySeconds", "20", None),
            ("BottingRetryMax", "6", None),
            ("BottingRetryBaseSeconds", "8", None),
            ("BottingPlayerGraceMinutes", "15", None),
            ("BottingDraftPlaceId", "", None),
            ("BottingDraftJobId", "", None),
            ("BottingDraftLaunchData", "", None),
            ("BottingDraftPlayerAccountId", "", None),
            ("BottingDraftPlayerAccountIds", "", None),
            ("BottingDraftSelectedUserIds", "", None),
        ];

        let general = ini.section("General");
        for (key, value, comment) in defaults {
            if !general.exists(key) {
                general.set(key, value, *comment);
            }
        }
        if !general.exists("EncryptionMethod") {
            general.set("EncryptionMethod", "default", None);
        }
        if !general.exists("ThemeWindowsNavbarAutoEnabledV1") {
            general.set("ThemeWindowsNavbar", "true", None);
            general.set("ThemeWindowsNavbarAutoEnabledV1", "true", None);
        }
        if !general.exists("EncryptionOnboardingState") {
            general.set(
                "EncryptionOnboardingState",
                if settings_file_existed {
                    "completed"
                } else {
                    "pending"
                },
                None,
            );
        }
        if !general.exists("FirstRunWalkthroughState") {
            general.set(
                "FirstRunWalkthroughState",
                if settings_file_existed {
                    "completed"
                } else {
                    "pending"
                },
                None,
            );
        }

        let developer = ini.section("Developer");
        if !developer.exists("DevMode") {
            developer.set("DevMode", "false", None);
        }
        if !developer.exists("EnableWebServer") {
            developer.set("EnableWebServer", "false", None);
        }
        if !developer.exists("IsTeleport") {
            developer.set("IsTeleport", "false", None);
        }
        if !developer.exists("UseOldJoin") {
            developer.set("UseOldJoin", "false", None);
        }

        let ws_defaults: &[(&str, &str)] = &[
            ("WebServerPort", "7963"),
            ("AllowGetCookie", "false"),
            ("AllowGetAccounts", "false"),
            ("AllowLaunchAccount", "false"),
            ("AllowAccountEditing", "false"),
            ("EveryRequestRequiresPassword", "false"),
            ("AllowExternalConnections", "false"),
        ];

        let webserver = ini.section("WebServer");
        for (key, value) in ws_defaults {
            if !webserver.exists(key) {
                webserver.set(key, value, None);
            }
        }

        let ac_defaults: &[(&str, &str)] = &[
            ("AllowExternalConnections", "false"),
            ("StartOnLaunch", "false"),
            ("RelaunchDelay", "60"),
            ("LauncherDelay", "9"),
            ("NexusPort", "5242"),
            ("AutoMinimizeEnabled", "false"),
            ("AutoCloseEnabled", "false"),
            ("InternetCheck", "false"),
            ("UsePresence", "false"),
            ("AutoMinimizeInterval", "15"),
            ("AutoCloseInterval", "5"),
            ("MaxInstances", "3"),
            ("AutoCloseType", "0"),
        ];

        let account_control = ini.section("AccountControl");
        for (key, value) in ac_defaults {
            if !account_control.exists(key) {
                account_control.set(key, value, None);
            }
        }

        let watcher_defaults: &[(&str, &str)] = &[
            ("Enabled", "false"),
            ("ScanInterval", "6"),
            ("ReadInterval", "250"),
            ("ExitIfNoConnection", "false"),
            ("NoConnectionTimeout", "60"),
            ("ExitOnBeta", "false"),
            ("CloseIfNotResponding", "false"),
            ("CloseRbxMemory", "false"),
            ("MemoryLowValue", "200"),
            ("CloseRbxWindowTitle", "false"),
            ("ExpectedWindowTitle", "Roblox"),
            ("SaveWindowPositions", "false"),
        ];

        let watcher = ini.section("Watcher");
        for (key, value) in watcher_defaults {
            if !watcher.exists(key) {
                watcher.set(key, value, None);
            }
        }

        let optimization_defaults: &[(&str, &str)] = &[
            ("NormalEnableProcessPolicy", "false"),
            ("NormalProcessPolicyDelayMs", "1500"),
            ("NormalPriorityClass", "normal"),
            ("NormalBackgroundMode", "false"),
            ("NormalEcoQos", "false"),
            ("NormalIgnoreTimerResolution", "false"),
            ("NormalMemoryPriority", "normal"),
            ("NormalEnableFastFlags", "false"),
            ("NormalFastFlagsJson", ""),
            ("NormalEnableJobCpuLimit", "false"),
            ("NormalJobCpuLimitPercent", "25"),
            ("NormalEnableJobMemoryLimit", "false"),
            ("NormalJobMemoryLimitMb", "2048"),
            ("BottingPlayerEnableProcessPolicy", "false"),
            ("BottingPlayerProcessPolicyDelayMs", "1500"),
            ("BottingPlayerPriorityClass", "normal"),
            ("BottingPlayerBackgroundMode", "false"),
            ("BottingPlayerEcoQos", "false"),
            ("BottingPlayerIgnoreTimerResolution", "false"),
            ("BottingPlayerMemoryPriority", "normal"),
            ("BottingPlayerEnableFastFlags", "false"),
            ("BottingPlayerFastFlagsJson", ""),
            ("BottingPlayerEnableJobCpuLimit", "false"),
            ("BottingPlayerJobCpuLimitPercent", "25"),
            ("BottingPlayerEnableJobMemoryLimit", "false"),
            ("BottingPlayerJobMemoryLimitMb", "2048"),
            ("BottingBotEnableProcessPolicy", "false"),
            ("BottingBotProcessPolicyDelayMs", "1500"),
            ("BottingBotPriorityClass", "below_normal"),
            ("BottingBotBackgroundMode", "true"),
            ("BottingBotEcoQos", "true"),
            ("BottingBotIgnoreTimerResolution", "true"),
            ("BottingBotMemoryPriority", "low"),
            ("BottingBotEnableFastFlags", "false"),
            ("BottingBotFastFlagsJson", ""),
            ("BottingBotEnableJobCpuLimit", "false"),
            ("BottingBotJobCpuLimitPercent", "20"),
            ("BottingBotEnableJobMemoryLimit", "false"),
            ("BottingBotJobMemoryLimitMb", "1536"),
            // Otimizacao que segue o foco (docs/features/performance.md):
            // opcional, desligada por padrao.
            ("FollowFocus", "false"),
            // Fundo mudo (feature `live-audio`; sem ela a chave e ignorada).
            ("MuteBackgroundClients", "false"),
        ];

        let optimization = ini.section("Optimization");
        for (key, value) in optimization_defaults {
            if !optimization.exists(key) {
                optimization.set(key, value, None);
            }
        }

        let linux_defaults: &[(&str, &str)] = &[
            ("PreferredRunner", "sober"),
            ("CustomLaunchCommand", ""),
            ("CustomProcessMatch", "sober,flatpak,roblox,robloxplayerbeta"),
            ("CustomLogDir", ""),
            ("EnableExperimentalMultiRbx", "false"),
            ("WindowControlBackend", "auto"),
        ];

        let linux = ini.section("Linux");
        for (key, value) in linux_defaults {
            if !linux.exists(key) {
                linux.set(key, value, None);
            }
        }

        let generator_defaults: &[(&str, &str)] = &[
            ("Provider", "bloxgen"),
            ("ExtraDelaySeconds", "1"),
            ("TargetGroup", "BloxGen"),
            ("MaxAccounts", "0"),
            ("MaxConsecutiveFailures", "3"),
            // Padrao de nome do fluxo gratis (formulario do Roblox): "arvore"
            // gera "arvore_k3p9z". Vazio mantem o nome de palavras de sempre.
            // Nao vale para o BloxGen, cujo nome vem pronto do provedor.
            ("SignupUsernamePrefix", ""),
            // Com prefixo, nomes em ordem (prefixo_1, prefixo_2...) em vez
            // dos 5 caracteres sorteados; o próximo número fica gravado.
            ("SignupUsernameNumbered", "false"),
            ("SignupUsernameNext", "1"),
            // O criador de contas clica em "Cadastrar-se" sozinho; o CAPTCHA,
            // se aparecer, continua sendo da pessoa (14/10/2026).
            ("SignupAutoSubmit", "true"),
        ];

        let generator = ini.section("Generator");
        for (key, value) in generator_defaults {
            if !generator.exists(key) {
                generator.set(key, value, None);
            }
        }

        let bloxgen_defaults: &[(&str, &str)] = &[
            ("Endpoint", "https://core.bloxgen.net"),
            ("ApiKey", ""),
            ("AccountType", "alt"),
        ];

        let bloxgen = ini.section("BloxGen");
        for (key, value) in bloxgen_defaults {
            if !bloxgen.exists(key) {
                bloxgen.set(key, value, None);
            }
        }

        let versions_defaults: &[(&str, &str)] = &[
            ("DefaultVersion", ""),
            ("MaxParallelDownloads", "4"),
            ("CatalogCacheMinutes", "10"),
            ("PreferOldJoinForVersioned", "true"),
            ("ShowPreReleaseVersions", "false"),
            ("AllowLaunchOnOpenVersion", "false"),
            ("AllowOutdated", "false"),
        ];

        let versions = ini.section("Versions");
        for (key, value) in versions_defaults {
            if !versions.exists(key) {
                versions.set(key, value, None);
            }
        }

        let isolation_defaults: &[(&str, &str)] = &[
            ("Mode", "Off"),
            ("SpoofMachineGuid", "false"),
            ("SpoofMacAddress", "false"),
            ("TargetAdapter", ""),
            ("IncludeStudio", "false"),
            ("PreserveFastFlags", "true"),
            ("PreserveBasicSettings", "true"),
            ("BackupMachineGuid", ""),
            ("BackupNetworkAddress", ""),
            ("BackupAdapterId", ""),
        ];

        let isolation = ini.section("Isolation");
        for (key, value) in isolation_defaults {
            if !isolation.exists(key) {
                isolation.set(key, value, None);
            }
        }

        let login_defaults: &[(&str, &str)] = &[
            // Anônimo por padrão (26/10/2026): cada uso do navegador começa num
            // perfil limpo e não guarda nada para a próxima vez.
            ("PersistentProfile", "false"),
            ("StealthMode", "true"),
            // Vazio = nada configurado; `IniSection::set` não grava valor em
            // branco, então esta chave só aparece no INI depois que o usuário
            // digita um caminho (ver EMPTY_STRING_DEFAULTS no teste abaixo).
            ("ManualBinaryPath", ""),
        ];

        let login = ini.section("Login");
        for (key, value) in login_defaults {
            if !login.exists(key) {
                login.set(key, value, None);
            }
        }
        // O padrão antigo era o perfil persistente, gravado no INI de quem já
        // usava o app: desliga uma vez (pedido do dono, 26/10/2026) e depois
        // respeita quem ligar de novo.
        if !login.exists("PrivateBrowserDefaultV1") {
            login.set("PersistentProfile", "false", None);
            login.set("PrivateBrowserDefaultV1", "true", None);
        }

        // AFK mode. `Key` nasce **vazia** de proposito: sem tecla escolhida pelo
        // usuario o modo nao liga, e chave vazia nao chega a ser gravada no INI
        // (`IniSection::set` trata valor em branco como remocao). `Mode` nasce
        // `key` (o modo que ja existia) e o ponto do modo clique no meio da janela.
        // O intervalo e `IntervalMinutes` + `IntervalSeconds`; os segundos nascem
        // `0`, entao quem ja usava o modo continua com o mesmo intervalo.
        let afk_defaults: &[(&str, &str)] = &[
            ("IntervalMinutes", "10"),
            ("IntervalSeconds", "0"),
            ("Key", ""),
            ("BeepOnCycle", "false"),
            ("BeepBeforeCycle", "false"),
            ("Mode", "key"),
            ("ClickX", "50"),
            ("ClickY", "50"),
            // Parada de emergência por chacoalhar o mouse: desligada por
            // padrão, escolha do dono (commands/shake_stop.rs).
            ("ShakeToStop", "false"),
        ];

        let afk = ini.section("Afk");
        for (key, value) in afk_defaults {
            if !afk.exists(key) {
                afk.set(key, value, None);
            }
        }

        // Gravações (docs/features/recordings.md): tocar a gravação da conta
        // depois de a reconexão automática devolvê-la ao jogo nasce desligado;
        // ligado, espera a conta ficar 30 s no jogo antes de tocar. O mesmo
        // para depois de um launch (ideia 52): desligado, 30 s.
        let recordings_defaults: &[(&str, &str)] = &[
            ("AfterReconnect", "false"),
            ("AfterReconnectDelaySeconds", "30"),
            ("AfterLaunch", "false"),
            ("AfterLaunchDelaySeconds", "30"),
        ];
        let recordings = ini.section("Recordings");
        for (key, value) in recordings_defaults {
            if !recordings.exists(key) {
                recordings.set(key, value, None);
            }
        }

        // Avisos no Discord (docs/features/webhook.md): **desligado** por
        // padrão — os avisos saem do PC. `Url` e `Mention` nascem vazias (e
        // por isso não chegam ao arquivo). Nome da conta mascarado por padrão.
        let webhook_defaults: &[(&str, &str)] = &[
            ("Enabled", "false"),
            ("NotifyDropped", "true"),
            ("NotifyReconnected", "false"),
            ("NotifyReconnectFailed", "true"),
            ("NotifyNotResponding", "true"),
            ("NotifyLaunchFailed", "false"),
            ("NotifyWrongAccount", "true"),
            ("NotifyModerated", "true"),
            ("NotifyBatchFinished", "false"),
            ("NotifyMemoryClosed", "true"),
            ("NotifySystemLowMemory", "false"),
            ("NotifyScriptMessage", "true"),
            ("MaskNames", "false"),
            ("QuietHours", "false"),
            ("QuietStart", "23:00"),
            ("QuietEnd", "08:00"),
            ("QuietSkip", "false"),
            ("StatusEnabled", "false"),
            ("StatusInterval", "5"),
            ("SummaryEnabled", "false"),
            ("SummaryTime", "23:00"),
        ];
        // Live (docs/features/live.md). `Accounts` vazio = todas as abertas.
        let live_defaults: &[(&str, &str)] = &[("ShowNames", "true"), ("KeepMinimized", "true"), ("Background", "dark")];
        let live = ini.section("Live");
        for (key, value) in live_defaults {
            if !live.exists(key) {
                live.set(key, value, None);
            }
        }

        let webhook = ini.section("Webhook");
        for (key, value) in webhook_defaults {
            if !webhook.exists(key) {
                webhook.set(key, value, None);
            }
        }

        // Automação do Roblox Player em Python (docs/features/python-automation.md):
        // o Modo desenvolvedor (editar os blocos padrão e o canal do terminal)
        // nasce desligado.
        let python = ini.section("Python");
        if !python.exists("DeveloperMode") {
            python.set("DeveloperMode", "false", None);
        }

        ini.section("Prompts");

        drop(ini);
        let _ = self.save();
    }

    pub fn save(&self) -> Result<(), String> {
        if self.load_failed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(
                "Settings file could not be read; refusing to overwrite it. Fix or restore RAMSettings.ini and restart.".to_string(),
            );
        }

        let ini = self.ini.lock().map_err(|e| e.to_string())?;
        ini.save(&self.file_path)
    }

    pub fn get_all(&self) -> Result<HashMap<String, HashMap<String, String>>, String> {
        let ini = self.ini.lock().map_err(|e| e.to_string())?;
        Ok(ini.to_map())
    }

    pub fn get(&self, section: &str, key: &str) -> Result<Option<String>, String> {
        let ini = self.ini.lock().map_err(|e| e.to_string())?;
        Ok(ini
            .get_section(section)
            .and_then(|s| s.get(key))
            .map(|v| v.to_string()))
    }

    pub fn get_bool(&self, section: &str, key: &str) -> bool {
        self.get(section, key)
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false)
    }

    pub fn get_int(&self, section: &str, key: &str) -> Option<i64> {
        self.get(section, key)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
    }

    pub fn get_float(&self, section: &str, key: &str) -> Option<f64> {
        self.get(section, key)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
    }

    pub fn get_string(&self, section: &str, key: &str) -> String {
        self.get(section, key).ok().flatten().unwrap_or_default()
    }

    pub fn set(&self, section: &str, key: &str, value: &str) -> Result<(), String> {
        let mut ini = self.ini.lock().map_err(|e| e.to_string())?;
        ini.section(section).set(key, value, None);
        drop(ini);
        self.save()
    }
}
