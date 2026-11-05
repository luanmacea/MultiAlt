//! Como o macOS abre o link do Roblox (branch `macos`, 04/11/2026).
//!
//! Parte pura, sem `cfg` de SO de propósito (como `sober.rs`): os testes rodam
//! no Windows. Quem usa de verdade é `platform/macos/launch.rs`.
//!
//! O problema que isto corrige: `open roblox-player:...` entrega o link ao
//! Roblox que **já está aberto** (o Launch Services manda o link para a
//! instância que existe), então a segunda conta trocava o jogo da primeira em
//! vez de abrir outro cliente. Com o Multi Roblox ligado e o app do Roblox
//! achado, o link vai por `open -n -a <Roblox.app> <link>`: `-n` força uma
//! instância nova, que recebe o link. O código herdado tentava isso só como
//! plano B, depois de um `open <link>` cujo `spawn` sempre dá certo — ou seja,
//! nunca. Sem o app achado, cai no `open <link>` com aviso (regra nº 1: abrir
//! do jeito que der).

/// Argumentos do `/usr/bin/open` para um link de launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPlan {
    pub args: Vec<String>,
    /// Aviso para o Console quando o plano não é o que o Multi Roblox pede.
    pub warning: Option<&'static str>,
}

pub fn open_plan(url: &str, multi_roblox: bool, roblox_app: Option<&str>) -> OpenPlan {
    match (multi_roblox, roblox_app) {
        (true, Some(app)) => OpenPlan {
            args: vec!["-n".into(), "-a".into(), app.into(), url.into()],
            warning: None,
        },
        (true, None) => OpenPlan {
            args: vec![url.into()],
            warning: Some(
                "Roblox.app was not found in /Applications or ~/Applications; opening the link without a new instance, so it may replace the account already open",
            ),
        },
        (false, _) => OpenPlan { args: vec![url.into()], warning: None },
    }
}

#[cfg(test)]
mod mac_open_tests {
    use super::*;

    const URL: &str = "roblox-player:1+launchmode:play";

    #[test]
    fn multi_roblox_opens_a_new_instance_of_the_roblox_app() {
        let plan = open_plan(URL, true, Some("/Applications/Roblox.app"));
        assert_eq!(plan.args, ["-n", "-a", "/Applications/Roblox.app", URL]);
        assert_eq!(plan.warning, None);
    }

    #[test]
    fn without_multi_roblox_the_link_goes_to_the_default_handler() {
        let plan = open_plan(URL, false, Some("/Applications/Roblox.app"));
        assert_eq!(plan.args, [URL]);
        assert_eq!(plan.warning, None);
    }

    #[test]
    fn multi_roblox_without_the_app_still_opens_and_warns() {
        let plan = open_plan(URL, true, None);
        assert_eq!(plan.args, [URL]);
        assert!(plan.warning.is_some());
    }
}
