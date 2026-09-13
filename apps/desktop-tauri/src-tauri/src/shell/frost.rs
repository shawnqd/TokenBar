//! Homemade flyout frost.
//!
//! GitHub flyouts that open instantly (FluentFlyout, Telegram, PowerToys
//! WinUI) never do expensive work on the show path — the window appears
//! first. WebView2 cannot host live Mica, so we fake the menu's frost
//! (14 DIP box blur, 0.82/0.78 tint) at 1/4 resolution on a worker thread.
//! Click only BitBlts a tiny snapshot; blur/BMP/emit happen after `show()`.

#![cfg(windows)]

use std::ffi::c_void;

use raw_window_handle::HasWindowHandle;
use tauri::WebviewWindow;

use crate::shell::popup_chrome::{BACKDROP_BLUR_DIP, backdrop_tint};
use crate::taskbar_widget::taskbar_is_light;

/// 1/4 linear — standard homemade acrylic downsample so the click path
/// only StretchBlts ~100×240, not a 400k-pixel 3-pass blur.
const SCALE: i32 = 4;
/// Menu card fill: light `#FFFFFF`, dark `#282828`.
const LIGHT_FILL: (f32, f32, f32) = (255.0, 255.0, 255.0);
const DARK_FILL: (f32, f32, f32) = (40.0, 40.0, 40.0);

#[repr(C)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_pels_per_meter: i32,
    y_pels_per_meter: i32,
    clr_used: u32,
    clr_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 3],
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetAncestor(hwnd: isize, flags: u32) -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut WinRect) -> i32;
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(hdc: isize) -> isize;
    fn CreateDIBSection(
        hdc: isize,
        info: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: isize,
        offset: u32,
    ) -> isize;
    fn SelectObject(hdc: isize, obj: isize) -> isize;
    fn DeleteObject(obj: isize) -> i32;
    fn DeleteDC(hdc: isize) -> i32;
    fn SetStretchBltMode(hdc: isize, mode: i32) -> i32;
    fn SetBrushOrgEx(hdc: isize, x: i32, y: i32, prev: *mut i32) -> i32;
    fn StretchBlt(
        dst: isize,
        dx: i32,
        dy: i32,
        dw: i32,
        dh: i32,
        src: isize,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        rop: u32,
    ) -> i32;
}

/// Cheap screen grab while the flyout is still hidden. Safe to run on the
/// UI thread — no blur, no BMP.
pub struct Snapshot {
    pixels: Vec<f32>,
    w: i32,
    h: i32,
    radius: i32,
    light: bool,
}

pub fn snapshot(win: &WebviewWindow) -> Option<Snapshot> {
    let hwnd = root_hwnd(win)?;
    let mut rect = WinRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let src_w = (rect.right - rect.left).max(1);
    let src_h = (rect.bottom - rect.top).max(1);
    let dw = (src_w / SCALE).max(8);
    let dh = (src_h / SCALE).max(8);
    let pixels = capture_scaled(rect.left, rect.top, src_w, src_h, dw, dh)?;
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let radius = ((BACKDROP_BLUR_DIP * dpi as i32) / (96 * SCALE)).max(1);
    Some(Snapshot {
        pixels,
        w: dw,
        h: dh,
        radius,
        light: surface_is_light(),
    })
}

/// Blur + tint + BMP. Runs off the UI thread.
pub fn finish(mut snap: Snapshot) -> Option<String> {
    blur_bgr(&mut snap.pixels, snap.w, snap.h, snap.radius);
    tint_bgr(&mut snap.pixels, snap.light);
    let bmp = encode_bmp(snap.w, snap.h, &snap.pixels);
    Some(format!(
        "url(\"data:image/bmp;base64,{}\")",
        base64_encode(&bmp)
    ))
}

fn root_hwnd(win: &WebviewWindow) -> Option<isize> {
    let handle = win.window_handle().ok()?;
    let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else {
        return None;
    };
    const GA_ROOT: u32 = 2;
    let inner = h.hwnd.get();
    let hwnd = unsafe { GetAncestor(inner, GA_ROOT) };
    Some(if hwnd != 0 { hwnd } else { inner })
}

fn surface_is_light() -> bool {
    match codexbar::settings::Settings::load().theme {
        codexbar::settings::ThemePreference::Light => true,
        codexbar::settings::ThemePreference::Dark => false,
        codexbar::settings::ThemePreference::Auto => taskbar_is_light(),
    }
}

fn capture_scaled(x: i32, y: i32, sw: i32, sh: i32, dw: i32, dh: i32) -> Option<Vec<f32>> {
    const SRCCOPY: u32 = 0x00CC_0020;
    const HALFTONE: i32 = 4;
    const DIB_RGB_COLORS: u32 = 0;
    let screen = unsafe { GetDC(0) };
    if screen == 0 {
        return None;
    }
    let info = BitmapInfo {
        header: BitmapInfoHeader {
            size: std::mem::size_of::<BitmapInfoHeader>() as u32,
            width: dw,
            height: -dh,
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: 0,
            x_pels_per_meter: 0,
            y_pels_per_meter: 0,
            clr_used: 0,
            clr_important: 0,
        },
        colors: [0; 3],
    };
    let hdc = unsafe { CreateCompatibleDC(screen) };
    if hdc == 0 {
        unsafe { ReleaseDC(0, screen) };
        return None;
    }
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bitmap = unsafe { CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, 0, 0) };
    if bitmap == 0 || bits.is_null() {
        unsafe {
            DeleteDC(hdc);
            ReleaseDC(0, screen);
        }
        return None;
    }
    let prev = unsafe { SelectObject(hdc, bitmap) };
    unsafe {
        SetStretchBltMode(hdc, HALFTONE);
        SetBrushOrgEx(hdc, 0, 0, std::ptr::null_mut());
    }
    let copied = unsafe { StretchBlt(hdc, 0, 0, dw, dh, screen, x, y, sw, sh, SRCCOPY) };
    let result = if copied == 0 {
        None
    } else {
        let src = unsafe { std::slice::from_raw_parts(bits as *const u8, (dw * dh * 4) as usize) };
        let mut out = vec![0.0f32; (dw * dh * 3) as usize];
        let mut sum = 0.0f64;
        for index in 0..(dw * dh) as usize {
            for channel in 0..3 {
                let value = src[index * 4 + channel] as f32;
                out[index * 3 + channel] = value;
                sum += value as f64;
            }
        }
        let mean = sum / (dw * dh * 3) as f64;
        if mean < 4.0 { None } else { Some(out) }
    };
    unsafe {
        SelectObject(hdc, prev);
        DeleteObject(bitmap);
        DeleteDC(hdc);
        ReleaseDC(0, screen);
    }
    result
}

fn blur_mask(mask: &mut [f32], w: i32, h: i32, radius: i32) {
    if radius <= 0 {
        return;
    }
    let mut scratch = vec![0.0f32; mask.len()];
    for _ in 0..3 {
        for y in 0..h {
            let base = (y * w) as usize;
            let mut sum = 0.0;
            for x in -radius..=radius {
                sum += mask[base + x.clamp(0, w - 1) as usize];
            }
            let scale = 1.0 / (2 * radius + 1) as f32;
            for x in 0..w {
                scratch[base + x as usize] = sum * scale;
                let out = (x - radius).clamp(0, w - 1) as usize;
                let inn = (x + radius + 1).clamp(0, w - 1) as usize;
                sum += mask[base + inn] - mask[base + out];
            }
        }
        let scale = 1.0 / (2 * radius + 1) as f32;
        for x in 0..w {
            let mut sum = 0.0;
            for y in -radius..=radius {
                sum += scratch[(y.clamp(0, h - 1) * w + x) as usize];
            }
            for y in 0..h {
                mask[(y * w + x) as usize] = sum * scale;
                let out = (y - radius).clamp(0, h - 1);
                let inn = (y + radius + 1).clamp(0, h - 1);
                sum += scratch[(inn * w + x) as usize] - scratch[(out * w + x) as usize];
            }
        }
    }
}

fn blur_bgr(backdrop: &mut [f32], w: i32, h: i32, radius: i32) {
    let count = (w * h) as usize;
    let mut plane = vec![0.0f32; count];
    for channel in 0..3 {
        for index in 0..count {
            plane[index] = backdrop[index * 3 + channel];
        }
        blur_mask(&mut plane, w, h, radius);
        for index in 0..count {
            backdrop[index * 3 + channel] = plane[index];
        }
    }
}

fn mix_tinted(back: f32, fill: f32, tint: f32) -> f32 {
    back + (fill - back) * tint
}

fn tint_bgr(pixels: &mut [f32], light: bool) {
    let tint = backdrop_tint(light);
    let (fill_b, fill_g, fill_r) = if light { LIGHT_FILL } else { DARK_FILL };
    for chunk in pixels.chunks_exact_mut(3) {
        chunk[0] = mix_tinted(chunk[0], fill_b, tint);
        chunk[1] = mix_tinted(chunk[1], fill_g, tint);
        chunk[2] = mix_tinted(chunk[2], fill_r, tint);
    }
}

fn encode_bmp(w: i32, h: i32, bgr: &[f32]) -> Vec<u8> {
    let row = (w * 4) as usize;
    let pixel_bytes = row * h as usize;
    let off = 54u32;
    let mut out = vec![0u8; off as usize + pixel_bytes];
    out[0] = b'B';
    out[1] = b'M';
    let file_size = out.len() as u32;
    out[2..6].copy_from_slice(&file_size.to_le_bytes());
    out[10..14].copy_from_slice(&off.to_le_bytes());
    out[14..18].copy_from_slice(&40u32.to_le_bytes());
    out[18..22].copy_from_slice(&w.to_le_bytes());
    out[22..26].copy_from_slice(&h.to_le_bytes());
    out[26..28].copy_from_slice(&1u16.to_le_bytes());
    out[28..30].copy_from_slice(&32u16.to_le_bytes());
    // Bottom-up BMP: Chromium is happiest with positive height.
    for y in 0..h {
        let src_y = h - 1 - y;
        for x in 0..w {
            let si = ((src_y * w + x) * 3) as usize;
            let di = off as usize + (y as usize * row) + (x as usize * 4);
            out[di] = bgr[si] as u8;
            out[di + 1] = bgr[si + 1] as u8;
            out[di + 2] = bgr[si + 2] as u8;
            out[di + 3] = 255;
        }
    }
    out
}

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | data[i + 2] as u32;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    if i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if i + 1 < data.len() {
            out.push(T[((n >> 6) & 63) as usize] as char);
            out.push('=');
        } else {
            out.push('=');
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_keeps_click_path_cheap() {
        assert_eq!(SCALE, 4);
    }

    #[test]
    fn mix_matches_menu_tint() {
        let light = mix_tinted(0.0, 255.0, backdrop_tint(true));
        assert!((light - 255.0 * 0.82).abs() < 0.01);
        let dark = mix_tinted(0.0, 40.0, backdrop_tint(false));
        assert!((dark - 40.0 * 0.78).abs() < 0.01);
    }

    #[test]
    fn bmp_header_is_windows_bitmap() {
        let pixels = vec![
            10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 11.0, 22.0, 33.0,
        ];
        let bmp = encode_bmp(2, 2, &pixels);
        assert_eq!(&bmp[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 54);
        assert_eq!(i32::from_le_bytes(bmp[18..22].try_into().unwrap()), 2);
        assert_eq!(bmp.last().copied(), Some(255));
    }

    #[test]
    fn base64_roundtrip_alphabet() {
        let s = base64_encode(b"frost");
        assert!(
            s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        );
        assert!(!s.is_empty());
    }
}
