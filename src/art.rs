//! Procedural artwork: a coffee mug. Shared by the app (tray icons) and build.rs (exe icon),
//! so it is drawn crisply at whatever size Windows asks for.
#![allow(dead_code)]

use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    /// Gray, cold mug.
    Off,
    /// Amber mug with steam.
    Awake,
    /// Amber mug with steam and a green "active" badge.
    Activity,
}

type Rgb = (u8, u8, u8);

const AMBER: Rgb = (0xF5, 0x9E, 0x0B);
const COFFEE: Rgb = (0x5B, 0x34, 0x14);
const GRAY: Rgb = (0x9C, 0xA3, 0xAF);
const GRAY_DARK: Rgb = (0x4B, 0x55, 0x63);
const GREEN: Rgb = (0x22, 0xC5, 0x5E);

fn dist(u: f32, v: f32, cx: f32, cy: f32) -> f32 {
    ((u - cx).powi(2) + (v - cy).powi(2)).sqrt()
}

fn rounded_rect(u: f32, v: f32, (x0, y0, x1, y1): (f32, f32, f32, f32), r_top: f32, r_bottom: f32) -> bool {
    if u < x0 || u > x1 || v < y0 || v > y1 {
        return false;
    }
    let r = if v < (y0 + y1) / 2.0 { r_top } else { r_bottom };
    dist(u, v, u.clamp(x0 + r, x1 - r), v.clamp(y0 + r, y1 - r)) <= r
}

/// A wavy vertical stroke with round caps.
fn wisp(u: f32, v: f32, xc: f32, phase: f32, half_width: f32) -> bool {
    let (y0, y1, amp) = (0.07, 0.33, 0.035);
    let x_at = |y: f32| xc + amp * ((y - y0) / (y1 - y0) * 1.2 * PI + phase).sin();
    if (y0..=y1).contains(&v) {
        (u - x_at(v)).abs() <= half_width
    } else {
        let y = v.clamp(y0, y1);
        dist(u, v, x_at(y), y) <= half_width
    }
}

/// Color at unit coordinates (0..1, y down), or None for transparent.
fn sample(u: f32, v: f32, look: Look, small: bool) -> Option<Rgb> {
    let active = look != Look::Off;
    let (body, surface) = if active { (AMBER, COFFEE) } else { (GRAY, GRAY_DARK) };
    let mut c = None;

    let d = dist(u, v, 0.68, 0.63);
    if u > 0.6 && (0.09..=0.18).contains(&d) {
        c = Some(body); // handle
    }
    if rounded_rect(u, v, (0.10, 0.40, 0.70, 0.92), 0.05, 0.15) {
        c = Some(body);
    }
    if rounded_rect(u, v, (0.16, 0.44, 0.64, 0.52), 0.035, 0.035) {
        c = Some(surface);
    }
    if active {
        // Fewer, thicker wisps at tray sizes so they don't blur together.
        let wisps: &[(f32, f32)] = if small { &[(0.29, 0.0), (0.51, 1.6)] } else { &[(0.24, 0.0), (0.40, 1.4), (0.56, 2.8)] };
        let half_width = if small { 0.05 } else { 0.042 };
        if wisps.iter().any(|&(x, phase)| wisp(u, v, x, phase, half_width)) {
            c = Some(body);
        }
    }
    if look == Look::Activity {
        let d = dist(u, v, 0.80, 0.80);
        if d <= 0.25 {
            c = None; // transparent gap separates the badge on any background
        }
        if d <= 0.19 {
            c = Some(GREEN);
        }
    }
    c
}

/// Renders the mug as straight-alpha RGBA, `size * size * 4` bytes, rows top-down.
pub fn render(size: u32, look: Look) -> Vec<u8> {
    const SS: u32 = 4; // 4x4 supersampling for anti-aliasing
    let small = size <= 20;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for py in 0..size {
        for px in 0..size {
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let v = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    if let Some(c) = sample(u, v, look, small) {
                        r += c.0 as u32;
                        g += c.1 as u32;
                        b += c.2 as u32;
                        n += 1;
                    }
                }
            }
            if n > 0 {
                let i = ((py * size + px) * 4) as usize;
                out[i] = (r / n) as u8;
                out[i + 1] = (g / n) as u8;
                out[i + 2] = (b / n) as u8;
                out[i + 3] = (n * 255 / (SS * SS)) as u8;
            }
        }
    }
    out
}

/// Encodes the mug at the given sizes as a .ico file (32-bit BMP entries).
pub fn ico(sizes: &[u32], look: Look) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| bmp_entry(s, &render(s, look))).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, img) in sizes.iter().zip(&images) {
        let dim = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

fn bmp_entry(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_stride = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    for v in [40u32, size, size * 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(size * size * 4 + mask_stride * size).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for row in (0..size).rev() {
        for x in 0..size {
            let i = ((row * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    out.resize(out.len() + (mask_stride * size) as usize, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test -- --ignored` writes previews to target/preview.
    #[test]
    #[ignore]
    fn write_previews() {
        std::fs::create_dir_all("target/preview").unwrap();
        for (name, look) in [("off", Look::Off), ("awake", Look::Awake), ("activity", Look::Activity)] {
            let data = ico(&[16, 20, 24, 32, 48, 256], look);
            std::fs::write(format!("target/preview/{name}.ico"), data).unwrap();
        }
    }
}
