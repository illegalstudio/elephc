//! Purpose:
//! GD built-in bitmap text rendering for the image bridge: `imagestring`,
//! `imagestringup`, and (through the prelude) `imagechar`/`imagecharup`. Glyphs
//! come from the public-domain `font8x8` 8×8 set.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`,
//!   behind `imagestring`/`imagestringup`.
//!
//! Key details:
//! - Every built-in GD font (numbers 1–5) is approximated by the same 8×8 glyph
//!   cell, so `imagefontwidth`/`imagefontheight` are 8 and the font number does
//!   not change the size. Native GD's per-font 5–9×8–15 cells are not reproduced.
//! - `font8x8` packs each glyph as eight row bytes with the least-significant bit
//!   as the leftmost column. Horizontal text advances 8 px per character;
//!   `imagestringup` rotates the layout 90° counter-clockwise.
//! - Rendering honors the image's alpha-blending mode via the shared
//!   `draw::plot` helper.

use std::os::raw::c_char;

use crate::{ffi_guard, cstr_arg, images, unpack_color};

/// Renders a string with the built-in 8×8 font at `(x, y)`. When `vertical` is
/// set the layout is rotated 90° counter-clockwise (`imagestringup`). The `font`
/// number is accepted for API parity but does not change the cell size.
fn render_builtin(handle: i64, x: i64, y: i64, color: i64, text: &str, vertical: bool) {
    use font8x8::{UnicodeFonts, BASIC_FONTS};

    let mut guard = images().lock().unwrap();
    let Some(obj) = guard.get_mut(&handle) else {
        return;
    };
    let blending = obj.alpha_blending;
    let src = unpack_color(color);
    for (index, ch) in text.chars().enumerate() {
        let glyph = BASIC_FONTS.get(ch).unwrap_or([0u8; 8]);
        let i = index as i64;
        for row in 0..8i64 {
            let bits = glyph[row as usize];
            for col in 0..8i64 {
                if bits & (1 << col) == 0 {
                    continue;
                }
                let (px, py) = if vertical {
                    // 90° CCW rotation of the horizontal layout about (x, y).
                    (x + row, y - i * 8 - col)
                } else {
                    (x + i * 8 + col, y + row)
                };
                crate::draw::plot(&mut obj.img, blending, px, py, src);
            }
        }
    }
}

/// Draws a string horizontally with the built-in font (`imagestring`).
#[no_mangle]
pub unsafe extern "C" fn elephc_img_string(
    handle: i64,
    font: i64,
    x: i64,
    y: i64,
    color: i64,
    text: *const c_char,
) {
    ffi_guard((), move || unsafe {
        let _ = font;
        if let Some(text) = cstr_arg(text) {
            render_builtin(handle, x, y, color, text, false);
        }
    })
}

/// Draws a string vertically (rotated up) with the built-in font
/// (`imagestringup`).
#[no_mangle]
pub unsafe extern "C" fn elephc_img_string_up(
    handle: i64,
    font: i64,
    x: i64,
    y: i64,
    color: i64,
    text: *const c_char,
) {
    ffi_guard((), move || unsafe {
        let _ = font;
        if let Some(text) = cstr_arg(text) {
            render_builtin(handle, x, y, color, text, true);
        }
    })
}
