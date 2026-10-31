#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

// Parte pura do launch do Sober (Linux): sem cfg de SO de propósito, para os
// testes rodarem no Windows. Quem usa de verdade é `linux/`.
pub mod sober;

#[cfg(target_os = "linux")]
pub mod linux;
