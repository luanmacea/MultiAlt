// O inspetor da tela Python: mostra a captura da janela de uma conta (para
// copiar ponto e cor em frações) e salva recortes em `assets/` para o
// `find_image`. Só cliente aberto pelo MultiAlt; só lê pixels. O recorte sai
// do **último quadro mostrado** da conta (guardado aqui com um id), não de uma
// captura nova: a janela pode ter mudado entre o Capture e o arrastar.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::primitives::{Desk, RealDesk};
use super::types::{FracRect, InspectorFrame};

/// Largura máxima da imagem que vai para a tela (as frações não mudam).
pub const INSPECTOR_MAX_WIDTH: u32 = 1280;

/// O último quadro mostrado de cada conta, em resolução cheia.
#[derive(Default)]
pub struct FrameCache {
    seq: AtomicU64,
    frames: Mutex<HashMap<i64, (u64, u32, u32, Vec<u8>)>>,
}

const FRAME_GONE: &str = "That capture is no longer available. Capture again, then select the area.";

impl FrameCache {
    /// Captura, guarda como o quadro da conta e devolve o que a tela mostra.
    pub fn capture(&self, desk: &dyn Desk, user_id: i64) -> Result<InspectorFrame, String> {
        let (w, h, bgra) = capture_with(desk, user_id)?;
        let mut shown = frame(w, h, &bgra)?;
        let id = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        shown.frame_id = id;
        self.frames.lock().unwrap().insert(user_id, (id, w, h, bgra));
        Ok(shown)
    }

    /// Salva o recorte do quadro `frame_id` da conta (só o último dela vale).
    pub fn save(&self, user_id: i64, frame_id: u64, rect: &FracRect, path: &Path) -> Result<(), String> {
        let frames = self.frames.lock().unwrap();
        match frames.get(&user_id) {
            Some((id, w, h, bgra)) if *id == frame_id => save_crop(path, *w, *h, bgra, rect),
            _ => Err(FRAME_GONE.into()),
        }
    }
}

/// Os quadros do inspetor do app.
pub fn frames() -> &'static FrameCache {
    static FRAMES: std::sync::LazyLock<FrameCache> = std::sync::LazyLock::new(FrameCache::default);
    &FRAMES
}

pub fn capture_frame(user_id: i64) -> Result<InspectorFrame, String> {
    frames().capture(&RealDesk, user_id)
}

fn capture_with(desk: &dyn Desk, user_id: i64) -> Result<(u32, u32, Vec<u8>), String> {
    let hwnd = desk
        .window_of(user_id)
        .ok_or("This account has no Roblox client opened by MultiAlt.")?;
    if desk.is_minimized(hwnd) {
        return Err("The window is minimized. Restore it and try again.".into());
    }
    desk.capture(hwnd)
}

/// Reduz (vizinho mais próximo) para no máximo `max_w` de largura e devolve RGB.
fn to_rgb_scaled(w: u32, h: u32, bgra: &[u8], max_w: u32) -> (u32, u32, Vec<u8>) {
    let (ow, oh) = if w > max_w {
        (max_w, ((h as u64 * max_w as u64) / w as u64).max(1) as u32)
    } else {
        (w, h)
    };
    let mut out = Vec::with_capacity((ow * oh * 3) as usize);
    for y in 0..oh {
        let sy = (y as u64 * h as u64 / oh as u64) as u32;
        for x in 0..ow {
            let sx = (x as u64 * w as u64 / ow as u64) as u32;
            let i = ((sy * w + sx) * 4) as usize;
            out.extend_from_slice(&[bgra[i + 2], bgra[i + 1], bgra[i]]);
        }
    }
    (ow, oh, out)
}

fn encode_png(w: u32, h: u32, data: &[u8], color: png::ColorType) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(data).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

pub fn frame(w: u32, h: u32, bgra: &[u8]) -> Result<InspectorFrame, String> {
    use base64::Engine;
    let (ow, oh, rgb) = to_rgb_scaled(w, h, bgra, INSPECTOR_MAX_WIDTH);
    let png = encode_png(ow, oh, &rgb, png::ColorType::Rgb)?;
    Ok(InspectorFrame {
        width: ow,
        height: oh,
        png_base64: base64::engine::general_purpose::STANDARD.encode(png),
        frame_id: 0,
    })
}

/// `assets/<name>`: o nome vem com ".png" (a tela já põe); sem extensão, ganha
/// ".png". Só nome, nunca caminho.
pub fn asset_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    let file = if name.contains('.') { name.to_string() } else { format!("{name}.png") };
    let stem = file
        .strip_suffix(".png")
        .ok_or_else(|| format!("Invalid image name \"{name}\": it must end with .png"))?;
    let ok = !stem.is_empty()
        && stem.len() <= 64
        && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !ok {
        return Err(format!(
            "Invalid image name \"{name}\": use 1-64 letters, digits, '_' or '-'"
        ));
    }
    Ok(root.join(super::workspace::ASSETS_DIR).join(file))
}

/// O recorte (frações) em resolução cheia, em PNG RGBA.
pub fn save_crop(path: &Path, w: u32, h: u32, bgra: &[u8], rect: &FracRect) -> Result<(), String> {
    let valid = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
    if !(valid(rect.x) && valid(rect.y) && valid(rect.w) && valid(rect.h)) || rect.w <= 0.0 || rect.h <= 0.0 {
        return Err("The selection must be inside the window".into());
    }
    let x0 = ((rect.x * w as f64).floor() as u32).min(w - 1);
    let y0 = ((rect.y * h as f64).floor() as u32).min(h - 1);
    let cw = ((rect.w * w as f64).round() as u32).clamp(1, w - x0);
    let ch = ((rect.h * h as f64).round() as u32).clamp(1, h - y0);
    let mut rgba = Vec::with_capacity((cw * ch * 4) as usize);
    for y in y0..y0 + ch {
        for x in x0..x0 + cw {
            let i = ((y * w + x) * 4) as usize;
            rgba.extend_from_slice(&[bgra[i + 2], bgra[i + 1], bgra[i], 255]);
        }
    }
    let png = encode_png(cw, ch, &rgba, png::ColorType::Rgba)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, png).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod python_inspector_tests {
    use super::*;
    use crate::python::primitives::python_primitives_tests::FakeDesk;

    fn image(w: u32, h: u32) -> Vec<u8> {
        (0..w * h).flat_map(|i| [(i % 256) as u8, 0, 200, 255]).collect()
    }

    #[test]
    fn only_tracked_clients_are_captured() {
        let desk = FakeDesk::default();
        assert!(capture_with(&desk, 1).unwrap_err().contains("no Roblox client opened by MultiAlt"));
        desk.windows.lock().unwrap().insert(1, 11);
        desk.minimized.lock().unwrap().insert(11);
        assert!(capture_with(&desk, 1).unwrap_err().contains("minimized"));
    }

    #[test]
    fn the_frame_is_a_png_scaled_to_the_maximum_width() {
        use base64::Engine;
        let f = frame(2000, 1000, &image(2000, 1000)).unwrap();
        assert_eq!((f.width, f.height), (INSPECTOR_MAX_WIDTH, 640));
        let bytes = base64::engine::general_purpose::STANDARD.decode(&f.png_base64).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let small = frame(10, 5, &image(10, 5)).unwrap();
        assert_eq!((small.width, small.height), (10, 5));
    }

    #[test]
    fn asset_names_are_validated_and_get_png() {
        let root = Path::new("C:/ws");
        assert_eq!(asset_path(root, "play_button.png").unwrap(), root.join("assets").join("play_button.png"));
        assert_eq!(asset_path(root, "play").unwrap(), root.join("assets").join("play.png"));
        for bad in ["../x.png", "a/b.png", "x.jpg", "", ".png", "a b.png", "x.png.exe"] {
            assert!(asset_path(root, bad).is_err(), "{bad:?}");
        }
    }

    /// O recorte sai da imagem que a pessoa viu, não de uma captura nova (a
    /// janela já pode ter mudado entre o Capture e o arrastar).
    #[test]
    fn the_asset_is_cut_from_the_frame_on_screen() {
        let desk = FakeDesk::default();
        desk.windows.lock().unwrap().insert(1, 11);
        let red: Vec<u8> = (0..4 * 4).flat_map(|_| [0u8, 0, 255, 255]).collect();
        let blue: Vec<u8> = (0..4 * 4).flat_map(|_| [255u8, 0, 0, 255]).collect();
        *desk.image.lock().unwrap() = Some((4, 4, red));
        let cache = FrameCache::default();
        let shown = cache.capture(&desk, 1).unwrap();
        // A janela mudou depois do Capture.
        *desk.image.lock().unwrap() = Some((4, 4, blue));
        let dir = std::env::temp_dir().join(format!(
            "ram-pyframe-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let path = asset_path(&dir, "btn").unwrap();
        let rect = FracRect { x: 0.0, y: 0.0, w: 0.5, h: 0.5 };
        cache.save(1, shown.frame_id, &rect, &path).unwrap();
        let decoder = png::Decoder::new(std::fs::File::open(&path).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(&buf[..4], &[255, 0, 0, 255], "red, as on screen (not the blue of a new capture)");
        // Um quadro que não é o último desta conta não serve mais.
        let newer = cache.capture(&desk, 1).unwrap();
        assert_ne!(newer.frame_id, shown.frame_id);
        assert!(cache.save(1, shown.frame_id, &rect, &path).unwrap_err().contains("Capture again"));
        assert!(cache.save(2, newer.frame_id, &rect, &path).unwrap_err().contains("Capture again"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crop_is_saved_at_full_resolution() {
        let dir = std::env::temp_dir().join(format!(
            "ram-pyasset-{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let path = asset_path(&dir, "btn").unwrap();
        save_crop(&path, 100, 50, &image(100, 50), &FracRect { x: 0.5, y: 0.5, w: 0.2, h: 0.2 }).unwrap();
        let decoder = png::Decoder::new(std::fs::File::open(&path).unwrap());
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (20, 10));
        assert!(save_crop(&path, 100, 50, &image(100, 50), &FracRect { x: 0.5, y: 0.5, w: 0.0, h: 0.2 }).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
