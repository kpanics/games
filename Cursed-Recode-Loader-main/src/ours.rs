//! Online clients in the current server: supplied by the authenticated presence worker.
pub use crate::client_marks::Client;

pub fn client_of(nick: &str, me: bool) -> Option<Client> {
    if me { return Some(Client::Recode); }
    if let Some(client) = crate::client_marks::client_of(nick) { return Some(client); }
    #[cfg(debug_assertions)]
    {
        use std::sync::OnceLock;
        static DEV: OnceLock<std::collections::HashMap<String, Client>> = OnceLock::new();
        return DEV.get_or_init(|| std::env::var("CURSED_DEV_OURS").unwrap_or_default().split(',')
            .filter_map(|p| p.split_once(':'))
            .filter_map(|(n,c)| Some((n.trim().to_ascii_lowercase(), Client::parse(c.trim())?))).collect())
            .get(&nick.to_ascii_lowercase()).copied();
    }
    #[cfg(not(debug_assertions))]
    None
}

fn wings() -> Option<([u32; 2], Vec<u8>)> {
    let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(include_bytes!("../assets/icons/cursed-wings.webp").as_slice())).ok()?;
    let (w, h) = d.dimensions();
    let mut pixels = vec![0; d.output_buffer_size()?];
    d.read_image(&mut pixels).ok()?;
    let rgba = if d.has_alpha() { pixels } else { pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect() };
    Some(([w, h], rgba))
}

/// The original marks of both clients, in the same square beside a player's name.
pub fn draw(c: &mut crate::gui::draw::Canvas, r: [f32; 4], client: Client, a: f32) {
    use crate::gui::draw::grey;
    match client {
        Client::Recode => c.logo(r, grey(1.0, 0.95 * a)),
        Client::Cursed => {
            use crate::gui::pics;
            const KEY: &str = "builtin:cursed-wings";
            if !pics::has(KEY) {
                use std::sync::OnceLock;
                static WINGS: OnceLock<Option<([u32; 2], Vec<u8>)>> = OnceLock::new();
                if let Some(([w, h], pixels)) = WINGS.get_or_init(wings) {
                    pics::put(KEY, *w, *h, pixels.clone());
                }
            }
            if let Some((uv, _)) = pics::get(KEY) {
                c.pic(r, uv, 0.95 * a, false, 1.0, 1.0, 0.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_brand_assets_have_valid_pixels() {
        let ([w, h], rgba) = wings().expect("embedded Cursed wings");
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert!(rgba.chunks_exact(4).any(|p| p[3] != 0));
        assert_eq!(include_bytes!("../assets/launch/logo_base.r8").len(), 192 * 192);
    }
}
