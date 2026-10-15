// Menu do ícone da bandeja: Show, "Launch preset" (um item por preset) e Quit.
// Ver docs/features/ui-layout.md ("Bandeja") e docs/features/presets.md.
//
// - Os itens levam **só o nome do preset** — nunca conta (o modo "Names
//   hidden" vale também aqui, e o menu da bandeja aparece por cima de tudo).
// - Clicar num preset chama o mesmo caminho do botão Launch da tela
//   (`launch_preset_with` → `run_launch_preset`), sem a pergunta de "conta
//   online" (a tela pode estar escondida; é como o horário).
// - O submenu é refeito quando os presets mudam: salvar, apagar, restaurar
//   backup (`refresh_tray_menu`).

const TRAY_ID: &str = "main";
const TRAY_SHOW_ID: &str = "show";
const TRAY_QUIT_ID: &str = "quit";
const TRAY_NO_PRESETS_ID: &str = "presets-none";
/// Prefixo do id de cada item de preset no menu: `preset:<id do preset>`.
const TRAY_PRESET_PREFIX: &str = "preset:";
/// Nome comprido no menu: corta aqui (com "…").
const TRAY_PRESET_LABEL_CHARS: usize = 40;
/// Mais que isto não cabe num menu de bandeja; o resto fica na tela.
const TRAY_MAX_PRESETS: usize = 30;

/// Um item do submenu "Launch preset".
#[derive(Debug, Clone, PartialEq, Eq)]
struct TrayPresetEntry {
    menu_id: String,
    label: String,
}

/// Texto do item: o nome do preset, cortado e com `&` dobrado (no Windows um
/// `&` sozinho vira atalho de teclado e some do texto).
fn tray_preset_label(name: &str) -> String {
    let name = name.trim();
    let mut label: String = name.chars().take(TRAY_PRESET_LABEL_CHARS).collect();
    if name.chars().count() > TRAY_PRESET_LABEL_CHARS {
        label.push('…');
    }
    label.replace('&', "&&")
}

/// O modelo do submenu (puro, testado sem janela): um item por preset, na
/// ordem da tela, até `TRAY_MAX_PRESETS`. Lista vazia = o menu mostra
/// "No presets" desabilitado.
fn tray_preset_entries(presets: &[LaunchPreset]) -> Vec<TrayPresetEntry> {
    presets
        .iter()
        .filter(|p| !p.id.is_empty())
        .take(TRAY_MAX_PRESETS)
        .map(|p| TrayPresetEntry {
            menu_id: format!("{TRAY_PRESET_PREFIX}{}", p.id),
            label: tray_preset_label(&p.name),
        })
        .collect()
}

/// O id do preset de um item do menu, ou `None` se o item não é de preset.
fn preset_id_from_menu_id(menu_id: &str) -> Option<&str> {
    menu_id.strip_prefix(TRAY_PRESET_PREFIX).filter(|id| !id.is_empty())
}

fn build_tray_menu(app: &AppHandle<Wry>) -> tauri::Result<tauri::menu::Menu<Wry>> {
    use tauri::menu::SubmenuBuilder;
    let presets = app.state::<LaunchPresetStore>().list().unwrap_or_default();
    let entries = tray_preset_entries(&presets);

    let mut submenu = SubmenuBuilder::new(app, "Launch preset");
    if entries.is_empty() {
        let none = MenuItemBuilder::with_id(TRAY_NO_PRESETS_ID, "No presets")
            .enabled(false)
            .build(app)?;
        submenu = submenu.item(&none);
    } else {
        for entry in &entries {
            let item = MenuItemBuilder::with_id(entry.menu_id.as_str(), entry.label.as_str()).build(app)?;
            submenu = submenu.item(&item);
        }
    }
    let submenu = submenu.build()?;

    let show = MenuItemBuilder::with_id(TRAY_SHOW_ID, "Show").build(app)?;
    let quit = MenuItemBuilder::with_id(TRAY_QUIT_ID, "Quit").build(app)?;
    MenuBuilder::new(app)
        .item(&show)
        .item(&submenu)
        .separator()
        .item(&quit)
        .build()
}

/// Refaz o menu da bandeja (presets mudaram). Falha só vai para o log: o menu
/// antigo continua funcionando, e um preset que sumiu responde "no longer exists".
fn refresh_tray_menu(app: &AppHandle<Wry>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build_tray_menu(app) {
        Ok(menu) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                eprintln!("Tray menu refresh failed: {e}");
            }
        }
        Err(e) => eprintln!("Tray menu build failed: {e}"),
    }
}

fn show_main_window(app: &AppHandle<Wry>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn handle_tray_menu_event(app: &AppHandle<Wry>, menu_id: &str) {
    match menu_id {
        TRAY_SHOW_ID => show_main_window(app),
        TRAY_QUIT_ID => app.exit(0),
        other => {
            if let Some(preset_id) = preset_id_from_menu_id(other) {
                let app = app.clone();
                let preset_id = preset_id.to_string();
                tauri::async_runtime::spawn(async move {
                    // O resultado vai para o Console e vira toast (evento
                    // `launch-preset` com `source: "tray"`).
                    let _ = launch_preset_with(&app, &preset_id, PresetTrigger::Tray).await;
                });
            }
        }
    }
}
