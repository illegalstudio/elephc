//! Purpose:
//! GD raster and handle-management primitives of the image bridge: creating
//! images, allocating GD packed colors, setting pixels, querying size, the
//! true-color flag, the stored DPI resolution, and destroying handles.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`
//!   declarations, behind the procedural GD functions (`imagecreatetruecolor`,
//!   `imagecolorallocate`, `imagesetpixel`, `imagesx`, `imageistruecolor`,
//!   `imageresolution`, `imagedestroy`, …).
//!
//! Key details:
//! - All images are stored as 8-bit straight-alpha RGBA (`ImageObj`); the GD
//!   palette/truecolor distinction is carried only as a flag for
//!   `imageistruecolor`.
//! - Out-of-bounds coordinates and unknown handles are tolerated (no-op / `-1`),
//!   matching GD's lenient behavior, so a misuse never aborts the program.

use image::Rgba;

use crate::{ffi_guard, lock_recover, blend_over, images, insert_image, pack_color, try_new_rgba, unpack_color, ImageObj};

/// Creates a true-color RGBA image filled with opaque black (GD's default) and
/// returns its handle, or `-1` if the dimensions are invalid.
#[no_mangle]
pub extern "C" fn elephc_img_create_truecolor(width: i64, height: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(img) = try_new_rgba(width, height, Rgba([0, 0, 0, 255])) else {
            return -1;
        };
        insert_image(ImageObj::new(img, true))
    })
}

/// Creates a palette-style image and returns its handle. The buffer is RGBA like
/// every image, filled transparent; only the `truecolor = false` flag
/// distinguishes it (so `imageistruecolor` reports `false`). Full palette
/// semantics are not modeled; the image stays RGBA.
#[no_mangle]
pub extern "C" fn elephc_img_create(width: i64, height: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(img) = try_new_rgba(width, height, Rgba([0, 0, 0, 0])) else {
            return -1;
        };
        insert_image(ImageObj::new(img, false))
    })
}

/// Returns the GD packed value for an opaque RGB color
/// (`(r << 16) | (g << 8) | b`). The image handle is taken for API parity with
/// GD (palette images allocate a real slot); every image is stored
/// true-color for color storage, so the handle is currently unused.
#[no_mangle]
pub extern "C" fn elephc_img_color_allocate(handle: i64, r: i64, g: i64, b: i64) -> i64 {
    ffi_guard(-1, move || {
        let _ = handle;
        ((r & 0xff) << 16) | ((g & 0xff) << 8) | (b & 0xff)
    })
}

/// Returns the GD packed value for an RGBA color, folding GD's 7-bit alpha into
/// bits 24-30 (`(a << 24) | (r << 16) | (g << 8) | b`). The image handle is taken
/// for API parity; see [`elephc_img_color_allocate`].
#[no_mangle]
pub extern "C" fn elephc_img_color_allocate_alpha(
    handle: i64,
    r: i64,
    g: i64,
    b: i64,
    a: i64,
) -> i64 {
    ffi_guard(-1, move || {
        let _ = handle;
        ((a & 0x7f) << 24) | ((r & 0xff) << 16) | ((g & 0xff) << 8) | (b & 0xff)
    })
}

/// Sets a single pixel to a GD packed color. When alpha blending is on, the
/// color is composited over the existing pixel; otherwise it overwrites
/// (preserving the source alpha). Out-of-bounds coordinates and unknown handles
/// are ignored, matching GD's tolerant behavior.
#[no_mangle]
pub extern "C" fn elephc_img_set_pixel(handle: i64, x: i64, y: i64, color: i64) {
    ffi_guard((), move || {
        if x < 0 || y < 0 {
            return;
        }
        let mut guard = lock_recover(images());
        if let Some(obj) = guard.get_mut(&handle) {
            if (x as u32) < obj.img.width() && (y as u32) < obj.img.height() {
                let src = unpack_color(color);
                let pixel = if obj.alpha_blending {
                    blend_over(src, *obj.img.get_pixel(x as u32, y as u32))
                } else {
                    src
                };
                obj.img.put_pixel(x as u32, y as u32, pixel);
            }
        }
    })
}

/// Returns the GD packed color of a pixel, or `-1` for an unknown handle or
/// out-of-bounds coordinate. Backs `imagecolorat`.
#[no_mangle]
pub extern "C" fn elephc_img_color_at(handle: i64, x: i64, y: i64) -> i64 {
    ffi_guard(-1, move || {
        if x < 0 || y < 0 {
            return -1;
        }
        match lock_recover(images()).get(&handle) {
            Some(obj) if (x as u32) < obj.img.width() && (y as u32) < obj.img.height() => {
                pack_color(*obj.img.get_pixel(x as u32, y as u32))
            }
            _ => -1,
        }
    })
}

/// Returns the image width in pixels, or `-1` for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_img_sx(handle: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(images()).get(&handle) {
            Some(obj) => obj.img.width() as i64,
            None => -1,
        }
    })
}

/// Returns the image height in pixels, or `-1` for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_img_sy(handle: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(images()).get(&handle) {
            Some(obj) => obj.img.height() as i64,
            None => -1,
        }
    })
}

/// Returns `1` if the image is true-color, `0` if it is a palette image, or `-1`
/// for an unknown handle. Backs `imageistruecolor`.
#[no_mangle]
pub extern "C" fn elephc_img_is_truecolor(handle: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(images()).get(&handle) {
            Some(obj) => obj.truecolor as i64,
            None => -1,
        }
    })
}

/// Returns the stored horizontal DPI resolution, or `-1` for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_img_res_x(handle: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(images()).get(&handle) {
            Some(obj) => obj.res_x,
            None => -1,
        }
    })
}

/// Returns the stored vertical DPI resolution, or `-1` for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_img_res_y(handle: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(images()).get(&handle) {
            Some(obj) => obj.res_y,
            None => -1,
        }
    })
}

/// Sets the stored DPI resolution. Unknown handles are ignored. Backs the
/// setter form of `imageresolution`.
#[no_mangle]
pub extern "C" fn elephc_img_set_res(handle: i64, res_x: i64, res_y: i64) {
    ffi_guard((), move || {
        if let Some(obj) = lock_recover(images()).get_mut(&handle) {
            obj.res_x = res_x;
            obj.res_y = res_y;
        }
    })
}

/// Sets the alpha-blending mode (`imagealphablending`). Unknown handles ignored.
#[no_mangle]
pub extern "C" fn elephc_img_set_alpha_blending(handle: i64, on: i64) {
    ffi_guard((), move || {
        if let Some(obj) = lock_recover(images()).get_mut(&handle) {
            obj.alpha_blending = on != 0;
        }
    })
}

/// Sets the save-alpha flag (`imagesavealpha`). Unknown handles ignored.
#[no_mangle]
pub extern "C" fn elephc_img_set_save_alpha(handle: i64, on: i64) {
    ffi_guard((), move || {
        if let Some(obj) = lock_recover(images()).get_mut(&handle) {
            obj.save_alpha = on != 0;
        }
    })
}

/// Sets the GD packed transparent color (`imagecolortransparent`). Unknown
/// handles ignored.
#[no_mangle]
pub extern "C" fn elephc_img_set_transparent(handle: i64, color: i64) {
    ffi_guard((), move || {
        if let Some(obj) = lock_recover(images()).get_mut(&handle) {
            obj.transparent = color;
        }
    })
}

/// Returns the stored transparent color, or `-1` for none/unknown handle.
#[no_mangle]
pub extern "C" fn elephc_img_get_transparent(handle: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(images()).get(&handle) {
            Some(obj) => obj.transparent,
            None => -1,
        }
    })
}

/// Returns the number of palette colors (`imagecolorstotal`): `0` for a
/// true-color image, or `-1` for an unknown handle. Palette images are modeled
/// as RGBA in elephc, so no fixed palette size is tracked.
#[no_mangle]
pub extern "C" fn elephc_img_color_total(handle: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(images()).get(&handle) {
            Some(_) => 0,
            None => -1,
        }
    })
}

/// Sets the true-color flag (`imagepalettetotruecolor` /
/// `imagetruecolortopalette`). Unknown handles ignored. elephc stores every
/// image as RGBA, so this only flips the flag without requantizing.
#[no_mangle]
pub extern "C" fn elephc_img_set_truecolor(handle: i64, on: i64) {
    ffi_guard((), move || {
        if let Some(obj) = lock_recover(images()).get_mut(&handle) {
            obj.truecolor = on != 0;
        }
    })
}

/// Destroys an image, freeing its buffer. Idempotent: destroying an unknown or
/// already-freed handle is a no-op, so explicit `imagedestroy()` plus the
/// `GdImage` destructor cannot double-free.
#[no_mangle]
pub extern "C" fn elephc_img_destroy(handle: i64) {
    ffi_guard((), move || {
        lock_recover(images()).remove(&handle);
    })
}
