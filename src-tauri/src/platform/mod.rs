#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

// Parte pura do launch do Sober (Linux): sem cfg de SO de propósito, para os
// testes rodarem no Windows. Quem usa de verdade é `linux/`.
pub mod sober;

#[cfg(target_os = "linux")]
pub mod linux;

// O "SO do rastreador" nos dois Unix com launch: o código de `commands/` que
// só precisa de `tracker()`, `kill_*` e do multi usa `platform::unix_host`.
#[cfg(target_os = "macos")]
pub use macos as unix_host;
#[cfg(target_os = "linux")]
pub use linux as unix_host;
