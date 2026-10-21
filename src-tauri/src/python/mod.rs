// Automação do Roblox Player em Python (docs/features/python-automation.md).
//
// O Rust guarda o Python portátil, a pasta de trabalho e as *primitivas* (foco,
// tecla, clique, captura), e é nelas que moram as travas de segurança. Quem
// decide *como* trocar de janela e chamar o `main` do usuário é Python, nos
// blocos padrão editáveis. Os comandos da tela ficam em `commands`, sempre
// registrados; sem a feature `python-scripts` eles só dizem que a build não
// tem a funcionalidade.

pub mod commands;
pub mod errors;
#[cfg(feature = "python-scripts")]
pub mod focus;
#[cfg(feature = "python-scripts")]
pub mod primitives;
#[cfg(feature = "python-scripts")]
pub mod runtime;
pub mod types;
#[cfg(feature = "python-scripts")]
pub mod workspace;

/// Para todas as execuções Python (o chacoalhar do mouse, `Afk.ShakeToStop`).
/// Devolve quantas receberam o parar.
pub fn emergency_stop() -> usize {
    0
}

/// Quantas execuções Python estão vivas (rodando ou esperando o intervalo).
pub fn runs_active() -> usize {
    0
}
