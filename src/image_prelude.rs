//! Purpose:
//! PHP image standard-library surface (GD, Exif/IPTC, Imagick, Gmagick, Cairo),
//! implemented in elephc-PHP on top of the pure-Rust `elephc_image` bridge.
//! Declares the `elephc_image` externs, the `IMAGETYPE_*` constants, the
//! `GdImage` object, and the procedural image functions, so the feature compiles
//! through the normal pipeline (functions, classes, destructors, C-ABI extern
//! calls) with no codegen intrinsics.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via
//!   `inject_if_used`, after include resolution and before name resolution.
//!
//! Key details:
//! - The prelude is injected only when the program references an image symbol
//!   (see `detect`), so non-image binaries never declare `elephc_image` externs
//!   and never link `-lelephc_image`.
//! - `GdImage` holds the bridge's opaque `int` handle and frees it in
//!   `__destruct`; `imagedestroy()` frees it explicitly. The bridge's destroy is
//!   idempotent, so the two paths cannot double-free.
//! - Implements the full PHP image surface: the always-available core
//!   (`getimagesize`, `image_type_to_mime_type`, `image_type_to_extension`); GD
//!   raster I/O for PNG/JPEG/GIF/BMP/WebP (`imagecreatefrom*`,
//!   `imagecreatefromstring`, and the `image{png,jpeg,gif,bmp,webp}` output
//!   family, file or in-memory/stdout); the GD info functions
//!   (`imageistruecolor`, `imageresolution`, `imagetypes`, `gd_info`); GD color,
//!   drawing, text, transforms, and filters; Exif/IPTC metadata; and the Imagick,
//!   Gmagick, and Cairo OOP surfaces plus the procedural `cairo_*` API. Binary
//!   blobs cross the boundary through the bridge's staging buffer / encode cell
//!   plus `ptr_write_string` / `ptr_read_string`, since extern `string` is
//!   NUL-terminated and cannot carry encoded image bytes.

use crate::parser::ast::{BinOp, CType, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    class, e_array, e_array_assoc, e_binop, e_bool, e_call, e_cast, e_class_const, e_const,
    e_float, e_index, e_instance_of, e_int, e_method_call, e_neg, e_new, e_null, e_null_coalesce,
    e_post_inc, e_prop, e_static_call, e_str, e_ternary, e_this, e_this_prop, e_var, extern_fn,
    function, internal_declarations, method, s_array_assign, s_array_push, s_assign, s_break,
    s_const, s_echo, s_expr, s_for, s_if, s_prop_array_push, s_prop_assign, s_return, s_switch,
    s_throw, s_try, s_while, t_array, t_class, t_mixed, t_nullable, t_union,
};

mod detect;

/// `elephc_img_create_truecolor` — transcribed from the PHP form.
fn decl_extern_elephc_img_create_truecolor() -> Stmt {
    extern_fn("elephc_img_create_truecolor", "elephc_image")
        .param("width", CType::Int)
        .param("height", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_create` — transcribed from the PHP form.
fn decl_extern_elephc_img_create() -> Stmt {
    extern_fn("elephc_img_create", "elephc_image")
        .param("width", CType::Int)
        .param("height", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_color_allocate` — transcribed from the PHP form.
fn decl_extern_elephc_img_color_allocate() -> Stmt {
    extern_fn("elephc_img_color_allocate", "elephc_image")
        .param("handle", CType::Int)
        .param("red", CType::Int)
        .param("green", CType::Int)
        .param("blue", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_color_allocate_alpha` — transcribed from the PHP form.
fn decl_extern_elephc_img_color_allocate_alpha() -> Stmt {
    extern_fn("elephc_img_color_allocate_alpha", "elephc_image")
        .param("handle", CType::Int)
        .param("red", CType::Int)
        .param("green", CType::Int)
        .param("blue", CType::Int)
        .param("alpha", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_set_pixel` — transcribed from the PHP form.
fn decl_extern_elephc_img_set_pixel() -> Stmt {
    extern_fn("elephc_img_set_pixel", "elephc_image")
        .param("handle", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_sx` — transcribed from the PHP form.
fn decl_extern_elephc_img_sx() -> Stmt {
    extern_fn("elephc_img_sx", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_sy` — transcribed from the PHP form.
fn decl_extern_elephc_img_sy() -> Stmt {
    extern_fn("elephc_img_sy", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_is_truecolor` — transcribed from the PHP form.
fn decl_extern_elephc_img_is_truecolor() -> Stmt {
    extern_fn("elephc_img_is_truecolor", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_res_x` — transcribed from the PHP form.
fn decl_extern_elephc_img_res_x() -> Stmt {
    extern_fn("elephc_img_res_x", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_res_y` — transcribed from the PHP form.
fn decl_extern_elephc_img_res_y() -> Stmt {
    extern_fn("elephc_img_res_y", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_set_res` — transcribed from the PHP form.
fn decl_extern_elephc_img_set_res() -> Stmt {
    extern_fn("elephc_img_set_res", "elephc_image")
        .param("handle", CType::Int)
        .param("res_x", CType::Int)
        .param("res_y", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_color_at` — transcribed from the PHP form.
fn decl_extern_elephc_img_color_at() -> Stmt {
    extern_fn("elephc_img_color_at", "elephc_image")
        .param("handle", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_set_alpha_blending` — transcribed from the PHP form.
fn decl_extern_elephc_img_set_alpha_blending() -> Stmt {
    extern_fn("elephc_img_set_alpha_blending", "elephc_image")
        .param("handle", CType::Int)
        .param("on", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_set_save_alpha` — transcribed from the PHP form.
fn decl_extern_elephc_img_set_save_alpha() -> Stmt {
    extern_fn("elephc_img_set_save_alpha", "elephc_image")
        .param("handle", CType::Int)
        .param("on", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_set_transparent` — transcribed from the PHP form.
fn decl_extern_elephc_img_set_transparent() -> Stmt {
    extern_fn("elephc_img_set_transparent", "elephc_image")
        .param("handle", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_get_transparent` — transcribed from the PHP form.
fn decl_extern_elephc_img_get_transparent() -> Stmt {
    extern_fn("elephc_img_get_transparent", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_color_total` — transcribed from the PHP form.
fn decl_extern_elephc_img_color_total() -> Stmt {
    extern_fn("elephc_img_color_total", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_set_truecolor` — transcribed from the PHP form.
fn decl_extern_elephc_img_set_truecolor() -> Stmt {
    extern_fn("elephc_img_set_truecolor", "elephc_image")
        .param("handle", CType::Int)
        .param("on", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_set_thickness` — transcribed from the PHP form.
fn decl_extern_elephc_img_set_thickness() -> Stmt {
    extern_fn("elephc_img_set_thickness", "elephc_image")
        .param("handle", CType::Int)
        .param("thickness", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_line` — transcribed from the PHP form.
fn decl_extern_elephc_img_line() -> Stmt {
    extern_fn("elephc_img_line", "elephc_image")
        .param("handle", CType::Int)
        .param("x1", CType::Int)
        .param("y1", CType::Int)
        .param("x2", CType::Int)
        .param("y2", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_dashed_line` — transcribed from the PHP form.
fn decl_extern_elephc_img_dashed_line() -> Stmt {
    extern_fn("elephc_img_dashed_line", "elephc_image")
        .param("handle", CType::Int)
        .param("x1", CType::Int)
        .param("y1", CType::Int)
        .param("x2", CType::Int)
        .param("y2", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_rectangle` — transcribed from the PHP form.
fn decl_extern_elephc_img_rectangle() -> Stmt {
    extern_fn("elephc_img_rectangle", "elephc_image")
        .param("handle", CType::Int)
        .param("x1", CType::Int)
        .param("y1", CType::Int)
        .param("x2", CType::Int)
        .param("y2", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_filled_rectangle` — transcribed from the PHP form.
fn decl_extern_elephc_img_filled_rectangle() -> Stmt {
    extern_fn("elephc_img_filled_rectangle", "elephc_image")
        .param("handle", CType::Int)
        .param("x1", CType::Int)
        .param("y1", CType::Int)
        .param("x2", CType::Int)
        .param("y2", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_ellipse` — transcribed from the PHP form.
fn decl_extern_elephc_img_ellipse() -> Stmt {
    extern_fn("elephc_img_ellipse", "elephc_image")
        .param("handle", CType::Int)
        .param("cx", CType::Int)
        .param("cy", CType::Int)
        .param("w", CType::Int)
        .param("h", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_filled_ellipse` — transcribed from the PHP form.
fn decl_extern_elephc_img_filled_ellipse() -> Stmt {
    extern_fn("elephc_img_filled_ellipse", "elephc_image")
        .param("handle", CType::Int)
        .param("cx", CType::Int)
        .param("cy", CType::Int)
        .param("w", CType::Int)
        .param("h", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_arc` — transcribed from the PHP form.
fn decl_extern_elephc_img_arc() -> Stmt {
    extern_fn("elephc_img_arc", "elephc_image")
        .param("handle", CType::Int)
        .param("cxy", CType::Int)
        .param("wh", CType::Int)
        .param("start", CType::Int)
        .param("end", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_filled_arc` — transcribed from the PHP form.
fn decl_extern_elephc_img_filled_arc() -> Stmt {
    extern_fn("elephc_img_filled_arc", "elephc_image")
        .param("handle", CType::Int)
        .param("cxy", CType::Int)
        .param("wh", CType::Int)
        .param("start", CType::Int)
        .param("end", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_fill` — transcribed from the PHP form.
fn decl_extern_elephc_img_fill() -> Stmt {
    extern_fn("elephc_img_fill", "elephc_image")
        .param("handle", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_fill_to_border` — transcribed from the PHP form.
fn decl_extern_elephc_img_fill_to_border() -> Stmt {
    extern_fn("elephc_img_fill_to_border", "elephc_image")
        .param("handle", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .param("border", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_poly_reset` — transcribed from the PHP form.
fn decl_extern_elephc_img_poly_reset() -> Stmt {
    extern_fn("elephc_img_poly_reset", "elephc_image")
        .returns(CType::Void)
        .build()
}

/// `elephc_img_poly_add` — transcribed from the PHP form.
fn decl_extern_elephc_img_poly_add() -> Stmt {
    extern_fn("elephc_img_poly_add", "elephc_image")
        .param("x", CType::Int)
        .param("y", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_poly_line` — transcribed from the PHP form.
fn decl_extern_elephc_img_poly_line() -> Stmt {
    extern_fn("elephc_img_poly_line", "elephc_image")
        .param("handle", CType::Int)
        .param("color", CType::Int)
        .param("closed", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_poly_fill` — transcribed from the PHP form.
fn decl_extern_elephc_img_poly_fill() -> Stmt {
    extern_fn("elephc_img_poly_fill", "elephc_image")
        .param("handle", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_string` — transcribed from the PHP form.
fn decl_extern_elephc_img_string() -> Stmt {
    extern_fn("elephc_img_string", "elephc_image")
        .param("handle", CType::Int)
        .param("font", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .param("color", CType::Int)
        .param("text", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_string_up` — transcribed from the PHP form.
fn decl_extern_elephc_img_string_up() -> Stmt {
    extern_fn("elephc_img_string_up", "elephc_image")
        .param("handle", CType::Int)
        .param("font", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .param("color", CType::Int)
        .param("text", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_destroy` — transcribed from the PHP form.
fn decl_extern_elephc_img_destroy() -> Stmt {
    extern_fn("elephc_img_destroy", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_stage_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_img_stage_ptr() -> Stmt {
    extern_fn("elephc_img_stage_ptr", "elephc_image")
        .param("len", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_img_create_from_stage` — transcribed from the PHP form.
fn decl_extern_elephc_img_create_from_stage() -> Stmt {
    extern_fn("elephc_img_create_from_stage", "elephc_image")
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_create_from_file` — transcribed from the PHP form.
fn decl_extern_elephc_img_create_from_file() -> Stmt {
    extern_fn("elephc_img_create_from_file", "elephc_image")
        .param("path", CType::Str)
        .param("expected_fmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_write_file` — transcribed from the PHP form.
fn decl_extern_elephc_img_write_file() -> Stmt {
    extern_fn("elephc_img_write_file", "elephc_image")
        .param("handle", CType::Int)
        .param("fmt", CType::Int)
        .param("path", CType::Str)
        .param("quality", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_encode` — transcribed from the PHP form.
fn decl_extern_elephc_img_encode() -> Stmt {
    extern_fn("elephc_img_encode", "elephc_image")
        .param("handle", CType::Int)
        .param("fmt", CType::Int)
        .param("quality", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_encoded_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_img_encoded_ptr() -> Stmt {
    extern_fn("elephc_img_encoded_ptr", "elephc_image")
        .returns(CType::Ptr)
        .build()
}

/// `elephc_img_encoded_len` — transcribed from the PHP form.
fn decl_extern_elephc_img_encoded_len() -> Stmt {
    extern_fn("elephc_img_encoded_len", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_img_encoded_clear` — transcribed from the PHP form.
fn decl_extern_elephc_img_encoded_clear() -> Stmt {
    extern_fn("elephc_img_encoded_clear", "elephc_image")
        .returns(CType::Void)
        .build()
}

/// `elephc_img_probe_file` — transcribed from the PHP form.
fn decl_extern_elephc_img_probe_file() -> Stmt {
    extern_fn("elephc_img_probe_file", "elephc_image")
        .param("path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_probe_stage` — transcribed from the PHP form.
fn decl_extern_elephc_img_probe_stage() -> Stmt {
    extern_fn("elephc_img_probe_stage", "elephc_image")
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_probe_width` — transcribed from the PHP form.
fn decl_extern_elephc_img_probe_width() -> Stmt {
    extern_fn("elephc_img_probe_width", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_img_probe_height` — transcribed from the PHP form.
fn decl_extern_elephc_img_probe_height() -> Stmt {
    extern_fn("elephc_img_probe_height", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_img_probe_type` — transcribed from the PHP form.
fn decl_extern_elephc_img_probe_type() -> Stmt {
    extern_fn("elephc_img_probe_type", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_img_probe_bits` — transcribed from the PHP form.
fn decl_extern_elephc_img_probe_bits() -> Stmt {
    extern_fn("elephc_img_probe_bits", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_img_probe_channels` — transcribed from the PHP form.
fn decl_extern_elephc_img_probe_channels() -> Stmt {
    extern_fn("elephc_img_probe_channels", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_img_fbuf_reset` — transcribed from the PHP form.
fn decl_extern_elephc_img_fbuf_reset() -> Stmt {
    extern_fn("elephc_img_fbuf_reset", "elephc_image")
        .returns(CType::Void)
        .build()
}

/// `elephc_img_fbuf_push` — transcribed from the PHP form.
fn decl_extern_elephc_img_fbuf_push() -> Stmt {
    extern_fn("elephc_img_fbuf_push", "elephc_image")
        .param("fixed16", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_copy` — transcribed from the PHP form.
fn decl_extern_elephc_img_copy() -> Stmt {
    extern_fn("elephc_img_copy", "elephc_image")
        .param("dst", CType::Int)
        .param("src", CType::Int)
        .param("dxy", CType::Int)
        .param("sxy", CType::Int)
        .param("swh", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_copy_merge` — transcribed from the PHP form.
fn decl_extern_elephc_img_copy_merge() -> Stmt {
    extern_fn("elephc_img_copy_merge", "elephc_image")
        .param("dst", CType::Int)
        .param("src", CType::Int)
        .param("dxy", CType::Int)
        .param("sxy", CType::Int)
        .param("swh", CType::Int)
        .param("pct", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_copy_merge_gray` — transcribed from the PHP form.
fn decl_extern_elephc_img_copy_merge_gray() -> Stmt {
    extern_fn("elephc_img_copy_merge_gray", "elephc_image")
        .param("dst", CType::Int)
        .param("src", CType::Int)
        .param("dxy", CType::Int)
        .param("sxy", CType::Int)
        .param("swh", CType::Int)
        .param("pct", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_copy_resized` — transcribed from the PHP form.
fn decl_extern_elephc_img_copy_resized() -> Stmt {
    extern_fn("elephc_img_copy_resized", "elephc_image")
        .param("dst", CType::Int)
        .param("src", CType::Int)
        .param("dxy", CType::Int)
        .param("sxy", CType::Int)
        .param("dwh", CType::Int)
        .param("swh", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_copy_resampled` — transcribed from the PHP form.
fn decl_extern_elephc_img_copy_resampled() -> Stmt {
    extern_fn("elephc_img_copy_resampled", "elephc_image")
        .param("dst", CType::Int)
        .param("src", CType::Int)
        .param("dxy", CType::Int)
        .param("sxy", CType::Int)
        .param("dwh", CType::Int)
        .param("swh", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_scale` — transcribed from the PHP form.
fn decl_extern_elephc_img_scale() -> Stmt {
    extern_fn("elephc_img_scale", "elephc_image")
        .param("src", CType::Int)
        .param("new_w", CType::Int)
        .param("new_h", CType::Int)
        .param("mode", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_crop` — transcribed from the PHP form.
fn decl_extern_elephc_img_crop() -> Stmt {
    extern_fn("elephc_img_crop", "elephc_image")
        .param("src", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .param("w", CType::Int)
        .param("h", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_crop_auto` — transcribed from the PHP form.
fn decl_extern_elephc_img_crop_auto() -> Stmt {
    extern_fn("elephc_img_crop_auto", "elephc_image")
        .param("src", CType::Int)
        .param("mode", CType::Int)
        .param("color", CType::Int)
        .param("threshold_permille", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_flip` — transcribed from the PHP form.
fn decl_extern_elephc_img_flip() -> Stmt {
    extern_fn("elephc_img_flip", "elephc_image")
        .param("handle", CType::Int)
        .param("mode", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_rotate` — transcribed from the PHP form.
fn decl_extern_elephc_img_rotate() -> Stmt {
    extern_fn("elephc_img_rotate", "elephc_image")
        .param("src", CType::Int)
        .param("angle_mdeg", CType::Int)
        .param("bgcolor", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_affine` — transcribed from the PHP form.
fn decl_extern_elephc_img_affine() -> Stmt {
    extern_fn("elephc_img_affine", "elephc_image")
        .param("src", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_filter` — transcribed from the PHP form.
fn decl_extern_elephc_img_filter() -> Stmt {
    extern_fn("elephc_img_filter", "elephc_image")
        .param("handle", CType::Int)
        .param("filter", CType::Int)
        .param("a1", CType::Int)
        .param("a2", CType::Int)
        .param("a3", CType::Int)
        .param("a4", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_convolution` — transcribed from the PHP form.
fn decl_extern_elephc_img_convolution() -> Stmt {
    extern_fn("elephc_img_convolution", "elephc_image")
        .param("handle", CType::Int)
        .param("div_fixed", CType::Int)
        .param("offset_fixed", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_gamma` — transcribed from the PHP form.
fn decl_extern_elephc_img_gamma() -> Stmt {
    extern_fn("elephc_img_gamma", "elephc_image")
        .param("handle", CType::Int)
        .param("in_fixed", CType::Int)
        .param("out_fixed", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_set_interpolation` — transcribed from the PHP form.
fn decl_extern_elephc_img_set_interpolation() -> Stmt {
    extern_fn("elephc_img_set_interpolation", "elephc_image")
        .param("handle", CType::Int)
        .param("method", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_get_interpolation` — transcribed from the PHP form.
fn decl_extern_elephc_img_get_interpolation() -> Stmt {
    extern_fn("elephc_img_get_interpolation", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_set_interlace` — transcribed from the PHP form.
fn decl_extern_elephc_img_set_interlace() -> Stmt {
    extern_fn("elephc_img_set_interlace", "elephc_image")
        .param("handle", CType::Int)
        .param("on", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_img_get_interlace` — transcribed from the PHP form.
fn decl_extern_elephc_img_get_interlace() -> Stmt {
    extern_fn("elephc_img_get_interlace", "elephc_image")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_in_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_img_in_ptr() -> Stmt {
    extern_fn("elephc_img_in_ptr", "elephc_image")
        .param("len", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_img_out_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_img_out_ptr() -> Stmt {
    extern_fn("elephc_img_out_ptr", "elephc_image")
        .returns(CType::Ptr)
        .build()
}

/// `elephc_img_kv_count` — transcribed from the PHP form.
fn decl_extern_elephc_img_kv_count() -> Stmt {
    extern_fn("elephc_img_kv_count", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_img_kv_key` — transcribed from the PHP form.
fn decl_extern_elephc_img_kv_key() -> Stmt {
    extern_fn("elephc_img_kv_key", "elephc_image")
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_img_kv_val` — transcribed from the PHP form.
fn decl_extern_elephc_img_kv_val() -> Stmt {
    extern_fn("elephc_img_kv_val", "elephc_image")
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_exif_read` — transcribed from the PHP form.
fn decl_extern_elephc_exif_read() -> Stmt {
    extern_fn("elephc_exif_read", "elephc_image")
        .param("path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_exif_tagname` — transcribed from the PHP form.
fn decl_extern_elephc_exif_tagname() -> Stmt {
    extern_fn("elephc_exif_tagname", "elephc_image")
        .param("number", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_exif_thumbnail` — transcribed from the PHP form.
fn decl_extern_elephc_exif_thumbnail() -> Stmt {
    extern_fn("elephc_exif_thumbnail", "elephc_image")
        .param("path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_exif_thumb_width` — transcribed from the PHP form.
fn decl_extern_elephc_exif_thumb_width() -> Stmt {
    extern_fn("elephc_exif_thumb_width", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_exif_thumb_height` — transcribed from the PHP form.
fn decl_extern_elephc_exif_thumb_height() -> Stmt {
    extern_fn("elephc_exif_thumb_height", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_exif_thumb_type` — transcribed from the PHP form.
fn decl_extern_elephc_exif_thumb_type() -> Stmt {
    extern_fn("elephc_exif_thumb_type", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_iptc_parse` — transcribed from the PHP form.
fn decl_extern_elephc_iptc_parse() -> Stmt {
    extern_fn("elephc_iptc_parse", "elephc_image")
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_iptc_key_count` — transcribed from the PHP form.
fn decl_extern_elephc_iptc_key_count() -> Stmt {
    extern_fn("elephc_iptc_key_count", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_iptc_key` — transcribed from the PHP form.
fn decl_extern_elephc_iptc_key() -> Stmt {
    extern_fn("elephc_iptc_key", "elephc_image")
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_iptc_val_count` — transcribed from the PHP form.
fn decl_extern_elephc_iptc_val_count() -> Stmt {
    extern_fn("elephc_iptc_val_count", "elephc_image")
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_iptc_val` — transcribed from the PHP form.
fn decl_extern_elephc_iptc_val() -> Stmt {
    extern_fn("elephc_iptc_val", "elephc_image")
        .param("key_index", CType::Int)
        .param("val_index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_iptc_embed` — transcribed from the PHP form.
fn decl_extern_elephc_iptc_embed() -> Stmt {
    extern_fn("elephc_iptc_embed", "elephc_image")
        .param("path", CType::Str)
        .param("in_len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_new` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_new() -> Stmt {
    extern_fn("elephc_imagick_new", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_destroy` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_destroy() -> Stmt {
    extern_fn("elephc_imagick_destroy", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_imagick_clear` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_clear() -> Stmt {
    extern_fn("elephc_imagick_clear", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_imagick_count` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_count() -> Stmt {
    extern_fn("elephc_imagick_count", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_read_file` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_read_file() -> Stmt {
    extern_fn("elephc_imagick_read_file", "elephc_image")
        .param("wand", CType::Int)
        .param("path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_read_blob` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_read_blob() -> Stmt {
    extern_fn("elephc_imagick_read_blob", "elephc_image")
        .param("wand", CType::Int)
        .param("len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_new_image` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_new_image() -> Stmt {
    extern_fn("elephc_imagick_new_image", "elephc_image")
        .param("wand", CType::Int)
        .param("w", CType::Int)
        .param("h", CType::Int)
        .param("bg", CType::Int)
        .param("fmt", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_add_image` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_add_image() -> Stmt {
    extern_fn("elephc_imagick_add_image", "elephc_image")
        .param("dst", CType::Int)
        .param("src", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_cur_width` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_cur_width() -> Stmt {
    extern_fn("elephc_imagick_cur_width", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_cur_height` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_cur_height() -> Stmt {
    extern_fn("elephc_imagick_cur_height", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_set_format` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_set_format() -> Stmt {
    extern_fn("elephc_imagick_set_format", "elephc_image")
        .param("wand", CType::Int)
        .param("fmt", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_imagick_get_format` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_get_format() -> Stmt {
    extern_fn("elephc_imagick_get_format", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_set_quality` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_set_quality() -> Stmt {
    extern_fn("elephc_imagick_set_quality", "elephc_image")
        .param("wand", CType::Int)
        .param("quality", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_imagick_get_quality` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_get_quality() -> Stmt {
    extern_fn("elephc_imagick_get_quality", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_write_file` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_write_file() -> Stmt {
    extern_fn("elephc_imagick_write_file", "elephc_image")
        .param("wand", CType::Int)
        .param("path", CType::Str)
        .param("fmt_override", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_get_blob` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_get_blob() -> Stmt {
    extern_fn("elephc_imagick_get_blob", "elephc_image")
        .param("wand", CType::Int)
        .param("fmt_override", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_get_index` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_get_index() -> Stmt {
    extern_fn("elephc_imagick_get_index", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_set_index` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_set_index() -> Stmt {
    extern_fn("elephc_imagick_set_index", "elephc_image")
        .param("wand", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_next` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_next() -> Stmt {
    extern_fn("elephc_imagick_next", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_previous` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_previous() -> Stmt {
    extern_fn("elephc_imagick_previous", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_first` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_first() -> Stmt {
    extern_fn("elephc_imagick_first", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_imagick_last` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_last() -> Stmt {
    extern_fn("elephc_imagick_last", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_imagick_pixel_color` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_pixel_color() -> Stmt {
    extern_fn("elephc_imagick_pixel_color", "elephc_image")
        .param("wand", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_fill` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_fill() -> Stmt {
    extern_fn("elephc_imagick_fill", "elephc_image")
        .param("wand", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_resize` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_resize() -> Stmt {
    extern_fn("elephc_imagick_resize", "elephc_image")
        .param("wand", CType::Int)
        .param("cols", CType::Int)
        .param("rows", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_scale` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_scale() -> Stmt {
    extern_fn("elephc_imagick_scale", "elephc_image")
        .param("wand", CType::Int)
        .param("cols", CType::Int)
        .param("rows", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_crop` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_crop() -> Stmt {
    extern_fn("elephc_imagick_crop", "elephc_image")
        .param("wand", CType::Int)
        .param("w", CType::Int)
        .param("h", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_rotate` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_rotate() -> Stmt {
    extern_fn("elephc_imagick_rotate", "elephc_image")
        .param("wand", CType::Int)
        .param("angle_mdeg", CType::Int)
        .param("bg", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_flip` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_flip() -> Stmt {
    extern_fn("elephc_imagick_flip", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_flop` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_flop() -> Stmt {
    extern_fn("elephc_imagick_flop", "elephc_image")
        .param("wand", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_blur` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_blur() -> Stmt {
    extern_fn("elephc_imagick_blur", "elephc_image")
        .param("wand", CType::Int)
        .param("sigma_milli", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_negate` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_negate() -> Stmt {
    extern_fn("elephc_imagick_negate", "elephc_image")
        .param("wand", CType::Int)
        .param("only_gray", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_modulate` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_modulate() -> Stmt {
    extern_fn("elephc_imagick_modulate", "elephc_image")
        .param("wand", CType::Int)
        .param("b", CType::Int)
        .param("s", CType::Int)
        .param("h", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_sharpen` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_sharpen() -> Stmt {
    extern_fn("elephc_imagick_sharpen", "elephc_image")
        .param("wand", CType::Int)
        .param("radius_milli", CType::Int)
        .param("sigma_milli", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_composite` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_composite() -> Stmt {
    extern_fn("elephc_imagick_composite", "elephc_image")
        .param("dst", CType::Int)
        .param("src", CType::Int)
        .param("op", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_imagick_convolve` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_convolve() -> Stmt {
    extern_fn("elephc_imagick_convolve", "elephc_image")
        .param("wand", CType::Int)
        .param("div_fixed", CType::Int)
        .param("offset_fixed", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_idraw_new` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_new() -> Stmt {
    extern_fn("elephc_idraw_new", "elephc_image")
        .returns(CType::Int)
        .build()
}

/// `elephc_idraw_destroy` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_destroy() -> Stmt {
    extern_fn("elephc_idraw_destroy", "elephc_image")
        .param("draw", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_clear` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_clear() -> Stmt {
    extern_fn("elephc_idraw_clear", "elephc_image")
        .param("draw", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_set_fill` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_set_fill() -> Stmt {
    extern_fn("elephc_idraw_set_fill", "elephc_image")
        .param("draw", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_set_stroke` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_set_stroke() -> Stmt {
    extern_fn("elephc_idraw_set_stroke", "elephc_image")
        .param("draw", CType::Int)
        .param("color", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_set_stroke_width` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_set_stroke_width() -> Stmt {
    extern_fn("elephc_idraw_set_stroke_width", "elephc_image")
        .param("draw", CType::Int)
        .param("width", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_get_fill` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_get_fill() -> Stmt {
    extern_fn("elephc_idraw_get_fill", "elephc_image")
        .param("draw", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_idraw_line` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_line() -> Stmt {
    extern_fn("elephc_idraw_line", "elephc_image")
        .param("draw", CType::Int)
        .param("x1", CType::Int)
        .param("y1", CType::Int)
        .param("x2", CType::Int)
        .param("y2", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_rectangle` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_rectangle() -> Stmt {
    extern_fn("elephc_idraw_rectangle", "elephc_image")
        .param("draw", CType::Int)
        .param("x1", CType::Int)
        .param("y1", CType::Int)
        .param("x2", CType::Int)
        .param("y2", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_circle` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_circle() -> Stmt {
    extern_fn("elephc_idraw_circle", "elephc_image")
        .param("draw", CType::Int)
        .param("ox", CType::Int)
        .param("oy", CType::Int)
        .param("px", CType::Int)
        .param("py", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_ellipse` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_ellipse() -> Stmt {
    extern_fn("elephc_idraw_ellipse", "elephc_image")
        .param("draw", CType::Int)
        .param("oxy", CType::Int)
        .param("rxy", CType::Int)
        .param("se", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_point` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_point() -> Stmt {
    extern_fn("elephc_idraw_point", "elephc_image")
        .param("draw", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_poly_reset` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_poly_reset() -> Stmt {
    extern_fn("elephc_idraw_poly_reset", "elephc_image")
        .param("draw", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_poly_point` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_poly_point() -> Stmt {
    extern_fn("elephc_idraw_poly_point", "elephc_image")
        .param("draw", CType::Int)
        .param("x", CType::Int)
        .param("y", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_idraw_polygon` — transcribed from the PHP form.
fn decl_extern_elephc_idraw_polygon() -> Stmt {
    extern_fn("elephc_idraw_polygon", "elephc_image")
        .param("draw", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_imagick_draw` — transcribed from the PHP form.
fn decl_extern_elephc_imagick_draw() -> Stmt {
    extern_fn("elephc_imagick_draw", "elephc_image")
        .param("wand", CType::Int)
        .param("draw", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_surface_create` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_surface_create() -> Stmt {
    extern_fn("elephc_cairo_surface_create", "elephc_image")
        .param("w", CType::Int)
        .param("h", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_surface_destroy` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_surface_destroy() -> Stmt {
    extern_fn("elephc_cairo_surface_destroy", "elephc_image")
        .param("s", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_surface_width` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_surface_width() -> Stmt {
    extern_fn("elephc_cairo_surface_width", "elephc_image")
        .param("s", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_surface_height` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_surface_height() -> Stmt {
    extern_fn("elephc_cairo_surface_height", "elephc_image")
        .param("s", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_surface_encode_png` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_surface_encode_png() -> Stmt {
    extern_fn("elephc_cairo_surface_encode_png", "elephc_image")
        .param("s", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_surface_write_png` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_surface_write_png() -> Stmt {
    extern_fn("elephc_cairo_surface_write_png", "elephc_image")
        .param("s", CType::Int)
        .param("path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_surface_create_from_png` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_surface_create_from_png() -> Stmt {
    extern_fn("elephc_cairo_surface_create_from_png", "elephc_image")
        .param("path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_create` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_create() -> Stmt {
    extern_fn("elephc_cairo_create", "elephc_image")
        .param("surface", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_destroy` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_destroy() -> Stmt {
    extern_fn("elephc_cairo_destroy", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_save` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_save() -> Stmt {
    extern_fn("elephc_cairo_save", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_restore` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_restore() -> Stmt {
    extern_fn("elephc_cairo_restore", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_set_source_rgba` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_set_source_rgba() -> Stmt {
    extern_fn("elephc_cairo_set_source_rgba", "elephc_image")
        .param("ctx", CType::Int)
        .param("packed", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_set_source_pattern` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_set_source_pattern() -> Stmt {
    extern_fn("elephc_cairo_set_source_pattern", "elephc_image")
        .param("ctx", CType::Int)
        .param("pattern", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_set_line_width` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_set_line_width() -> Stmt {
    extern_fn("elephc_cairo_set_line_width", "elephc_image")
        .param("ctx", CType::Int)
        .param("w", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_set_line_cap` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_set_line_cap() -> Stmt {
    extern_fn("elephc_cairo_set_line_cap", "elephc_image")
        .param("ctx", CType::Int)
        .param("cap", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_set_line_join` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_set_line_join() -> Stmt {
    extern_fn("elephc_cairo_set_line_join", "elephc_image")
        .param("ctx", CType::Int)
        .param("join", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_set_fill_rule` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_set_fill_rule() -> Stmt {
    extern_fn("elephc_cairo_set_fill_rule", "elephc_image")
        .param("ctx", CType::Int)
        .param("rule", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_move_to` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_move_to() -> Stmt {
    extern_fn("elephc_cairo_move_to", "elephc_image")
        .param("ctx", CType::Int)
        .param("p_xy", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_line_to` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_line_to() -> Stmt {
    extern_fn("elephc_cairo_line_to", "elephc_image")
        .param("ctx", CType::Int)
        .param("p_xy", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_curve_to` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_curve_to() -> Stmt {
    extern_fn("elephc_cairo_curve_to", "elephc_image")
        .param("ctx", CType::Int)
        .param("p1", CType::Int)
        .param("p2", CType::Int)
        .param("p3", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_rectangle` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_rectangle() -> Stmt {
    extern_fn("elephc_cairo_rectangle", "elephc_image")
        .param("ctx", CType::Int)
        .param("p_xy", CType::Int)
        .param("p_wh", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_arc` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_arc() -> Stmt {
    extern_fn("elephc_cairo_arc", "elephc_image")
        .param("ctx", CType::Int)
        .param("p_center", CType::Int)
        .param("radius_fx", CType::Int)
        .param("p_angles", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_arc_negative` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_arc_negative() -> Stmt {
    extern_fn("elephc_cairo_arc_negative", "elephc_image")
        .param("ctx", CType::Int)
        .param("p_center", CType::Int)
        .param("radius_fx", CType::Int)
        .param("p_angles", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_close_path` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_close_path() -> Stmt {
    extern_fn("elephc_cairo_close_path", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_new_path` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_new_path() -> Stmt {
    extern_fn("elephc_cairo_new_path", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_new_sub_path` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_new_sub_path() -> Stmt {
    extern_fn("elephc_cairo_new_sub_path", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_translate` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_translate() -> Stmt {
    extern_fn("elephc_cairo_translate", "elephc_image")
        .param("ctx", CType::Int)
        .param("p_xy", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_scale` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_scale() -> Stmt {
    extern_fn("elephc_cairo_scale", "elephc_image")
        .param("ctx", CType::Int)
        .param("p_sxsy", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_rotate` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_rotate() -> Stmt {
    extern_fn("elephc_cairo_rotate", "elephc_image")
        .param("ctx", CType::Int)
        .param("angle_mrad", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_set_matrix` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_set_matrix() -> Stmt {
    extern_fn("elephc_cairo_set_matrix", "elephc_image")
        .param("ctx", CType::Int)
        .param("p_ab", CType::Int)
        .param("p_cd", CType::Int)
        .param("p_ef", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_transform` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_transform() -> Stmt {
    extern_fn("elephc_cairo_transform", "elephc_image")
        .param("ctx", CType::Int)
        .param("p_ab", CType::Int)
        .param("p_cd", CType::Int)
        .param("p_ef", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_identity_matrix` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_identity_matrix() -> Stmt {
    extern_fn("elephc_cairo_identity_matrix", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_get_current_point_x` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_get_current_point_x() -> Stmt {
    extern_fn("elephc_cairo_get_current_point_x", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_get_current_point_y` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_get_current_point_y() -> Stmt {
    extern_fn("elephc_cairo_get_current_point_y", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_paint` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_paint() -> Stmt {
    extern_fn("elephc_cairo_paint", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_fill` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_fill() -> Stmt {
    extern_fn("elephc_cairo_fill", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_fill_preserve` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_fill_preserve() -> Stmt {
    extern_fn("elephc_cairo_fill_preserve", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_stroke` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_stroke() -> Stmt {
    extern_fn("elephc_cairo_stroke", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_stroke_preserve` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_stroke_preserve() -> Stmt {
    extern_fn("elephc_cairo_stroke_preserve", "elephc_image")
        .param("ctx", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_pattern_create_rgba` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_pattern_create_rgba() -> Stmt {
    extern_fn("elephc_cairo_pattern_create_rgba", "elephc_image")
        .param("packed", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_pattern_create_linear` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_pattern_create_linear() -> Stmt {
    extern_fn("elephc_cairo_pattern_create_linear", "elephc_image")
        .param("p0", CType::Int)
        .param("p1", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_pattern_create_radial` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_pattern_create_radial() -> Stmt {
    extern_fn("elephc_cairo_pattern_create_radial", "elephc_image")
        .param("p_c0", CType::Int)
        .param("r0_fx", CType::Int)
        .param("p_c1", CType::Int)
        .param("r1_fx", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_cairo_pattern_add_color_stop_rgba` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_pattern_add_color_stop_rgba() -> Stmt {
    extern_fn("elephc_cairo_pattern_add_color_stop_rgba", "elephc_image")
        .param("pattern", CType::Int)
        .param("offset_fx", CType::Int)
        .param("packed", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_cairo_pattern_destroy` — transcribed from the PHP form.
fn decl_extern_elephc_cairo_pattern_destroy() -> Stmt {
    extern_fn("elephc_cairo_pattern_destroy", "elephc_image")
        .param("pattern", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `IMAGETYPE_UNKNOWN` — transcribed from the PHP form.
fn decl_const_imagetype_unknown() -> Stmt {
    s_const("IMAGETYPE_UNKNOWN", e_int(0))
}

/// `IMAGETYPE_GIF` — transcribed from the PHP form.
fn decl_const_imagetype_gif() -> Stmt {
    s_const("IMAGETYPE_GIF", e_int(1))
}

/// `IMAGETYPE_JPEG` — transcribed from the PHP form.
fn decl_const_imagetype_jpeg() -> Stmt {
    s_const("IMAGETYPE_JPEG", e_int(2))
}

/// `IMAGETYPE_PNG` — transcribed from the PHP form.
fn decl_const_imagetype_png() -> Stmt {
    s_const("IMAGETYPE_PNG", e_int(3))
}

/// `IMAGETYPE_SWF` — transcribed from the PHP form.
fn decl_const_imagetype_swf() -> Stmt {
    s_const("IMAGETYPE_SWF", e_int(4))
}

/// `IMAGETYPE_PSD` — transcribed from the PHP form.
fn decl_const_imagetype_psd() -> Stmt {
    s_const("IMAGETYPE_PSD", e_int(5))
}

/// `IMAGETYPE_BMP` — transcribed from the PHP form.
fn decl_const_imagetype_bmp() -> Stmt {
    s_const("IMAGETYPE_BMP", e_int(6))
}

/// `IMAGETYPE_TIFF_II` — transcribed from the PHP form.
fn decl_const_imagetype_tiff_ii() -> Stmt {
    s_const("IMAGETYPE_TIFF_II", e_int(7))
}

/// `IMAGETYPE_TIFF_MM` — transcribed from the PHP form.
fn decl_const_imagetype_tiff_mm() -> Stmt {
    s_const("IMAGETYPE_TIFF_MM", e_int(8))
}

/// `IMAGETYPE_JPC` — transcribed from the PHP form.
fn decl_const_imagetype_jpc() -> Stmt {
    s_const("IMAGETYPE_JPC", e_int(9))
}

/// `IMAGETYPE_JP2` — transcribed from the PHP form.
fn decl_const_imagetype_jp2() -> Stmt {
    s_const("IMAGETYPE_JP2", e_int(10))
}

/// `IMAGETYPE_JPX` — transcribed from the PHP form.
fn decl_const_imagetype_jpx() -> Stmt {
    s_const("IMAGETYPE_JPX", e_int(11))
}

/// `IMAGETYPE_JB2` — transcribed from the PHP form.
fn decl_const_imagetype_jb2() -> Stmt {
    s_const("IMAGETYPE_JB2", e_int(12))
}

/// `IMAGETYPE_SWC` — transcribed from the PHP form.
fn decl_const_imagetype_swc() -> Stmt {
    s_const("IMAGETYPE_SWC", e_int(13))
}

/// `IMAGETYPE_IFF` — transcribed from the PHP form.
fn decl_const_imagetype_iff() -> Stmt {
    s_const("IMAGETYPE_IFF", e_int(14))
}

/// `IMAGETYPE_WBMP` — transcribed from the PHP form.
fn decl_const_imagetype_wbmp() -> Stmt {
    s_const("IMAGETYPE_WBMP", e_int(15))
}

/// `IMAGETYPE_XBM` — transcribed from the PHP form.
fn decl_const_imagetype_xbm() -> Stmt {
    s_const("IMAGETYPE_XBM", e_int(16))
}

/// `IMAGETYPE_ICO` — transcribed from the PHP form.
fn decl_const_imagetype_ico() -> Stmt {
    s_const("IMAGETYPE_ICO", e_int(17))
}

/// `IMAGETYPE_WEBP` — transcribed from the PHP form.
fn decl_const_imagetype_webp() -> Stmt {
    s_const("IMAGETYPE_WEBP", e_int(18))
}

/// `IMAGETYPE_AVIF` — transcribed from the PHP form.
fn decl_const_imagetype_avif() -> Stmt {
    s_const("IMAGETYPE_AVIF", e_int(19))
}

/// `IMAGETYPE_COUNT` — transcribed from the PHP form.
fn decl_const_imagetype_count() -> Stmt {
    s_const("IMAGETYPE_COUNT", e_int(20))
}

/// `IMG_GIF` — transcribed from the PHP form.
fn decl_const_img_gif() -> Stmt {
    s_const("IMG_GIF", e_int(1))
}

/// `IMG_JPG` — transcribed from the PHP form.
fn decl_const_img_jpg() -> Stmt {
    s_const("IMG_JPG", e_int(2))
}

/// `IMG_JPEG` — transcribed from the PHP form.
fn decl_const_img_jpeg() -> Stmt {
    s_const("IMG_JPEG", e_int(2))
}

/// `IMG_PNG` — transcribed from the PHP form.
fn decl_const_img_png() -> Stmt {
    s_const("IMG_PNG", e_int(4))
}

/// `IMG_WBMP` — transcribed from the PHP form.
fn decl_const_img_wbmp() -> Stmt {
    s_const("IMG_WBMP", e_int(8))
}

/// `IMG_XPM` — transcribed from the PHP form.
fn decl_const_img_xpm() -> Stmt {
    s_const("IMG_XPM", e_int(16))
}

/// `IMG_WEBP` — transcribed from the PHP form.
fn decl_const_img_webp() -> Stmt {
    s_const("IMG_WEBP", e_int(32))
}

/// `IMG_BMP` — transcribed from the PHP form.
fn decl_const_img_bmp() -> Stmt {
    s_const("IMG_BMP", e_int(64))
}

/// `IMG_TGA` — transcribed from the PHP form.
fn decl_const_img_tga() -> Stmt {
    s_const("IMG_TGA", e_int(128))
}

/// `IMG_AVIF` — transcribed from the PHP form.
fn decl_const_img_avif() -> Stmt {
    s_const("IMG_AVIF", e_int(256))
}

/// `IMG_EFFECT_REPLACE` — transcribed from the PHP form.
fn decl_const_img_effect_replace() -> Stmt {
    s_const("IMG_EFFECT_REPLACE", e_int(0))
}

/// `IMG_EFFECT_ALPHABLEND` — transcribed from the PHP form.
fn decl_const_img_effect_alphablend() -> Stmt {
    s_const("IMG_EFFECT_ALPHABLEND", e_int(1))
}

/// `IMG_EFFECT_NORMAL` — transcribed from the PHP form.
fn decl_const_img_effect_normal() -> Stmt {
    s_const("IMG_EFFECT_NORMAL", e_int(2))
}

/// `IMG_EFFECT_OVERLAY` — transcribed from the PHP form.
fn decl_const_img_effect_overlay() -> Stmt {
    s_const("IMG_EFFECT_OVERLAY", e_int(3))
}

/// `IMG_EFFECT_MULTIPLY` — transcribed from the PHP form.
fn decl_const_img_effect_multiply() -> Stmt {
    s_const("IMG_EFFECT_MULTIPLY", e_int(4))
}

/// `IMG_ARC_PIE` — transcribed from the PHP form.
fn decl_const_img_arc_pie() -> Stmt {
    s_const("IMG_ARC_PIE", e_int(0))
}

/// `IMG_ARC_CHORD` — transcribed from the PHP form.
fn decl_const_img_arc_chord() -> Stmt {
    s_const("IMG_ARC_CHORD", e_int(1))
}

/// `IMG_ARC_NOFILL` — transcribed from the PHP form.
fn decl_const_img_arc_nofill() -> Stmt {
    s_const("IMG_ARC_NOFILL", e_int(2))
}

/// `IMG_ARC_EDGED` — transcribed from the PHP form.
fn decl_const_img_arc_edged() -> Stmt {
    s_const("IMG_ARC_EDGED", e_int(4))
}

/// `IMG_FLIP_HORIZONTAL` — transcribed from the PHP form.
fn decl_const_img_flip_horizontal() -> Stmt {
    s_const("IMG_FLIP_HORIZONTAL", e_int(1))
}

/// `IMG_FLIP_VERTICAL` — transcribed from the PHP form.
fn decl_const_img_flip_vertical() -> Stmt {
    s_const("IMG_FLIP_VERTICAL", e_int(2))
}

/// `IMG_FLIP_BOTH` — transcribed from the PHP form.
fn decl_const_img_flip_both() -> Stmt {
    s_const("IMG_FLIP_BOTH", e_int(3))
}

/// `IMG_FILTER_NEGATE` — transcribed from the PHP form.
fn decl_const_img_filter_negate() -> Stmt {
    s_const("IMG_FILTER_NEGATE", e_int(0))
}

/// `IMG_FILTER_GRAYSCALE` — transcribed from the PHP form.
fn decl_const_img_filter_grayscale() -> Stmt {
    s_const("IMG_FILTER_GRAYSCALE", e_int(1))
}

/// `IMG_FILTER_BRIGHTNESS` — transcribed from the PHP form.
fn decl_const_img_filter_brightness() -> Stmt {
    s_const("IMG_FILTER_BRIGHTNESS", e_int(2))
}

/// `IMG_FILTER_CONTRAST` — transcribed from the PHP form.
fn decl_const_img_filter_contrast() -> Stmt {
    s_const("IMG_FILTER_CONTRAST", e_int(3))
}

/// `IMG_FILTER_COLORIZE` — transcribed from the PHP form.
fn decl_const_img_filter_colorize() -> Stmt {
    s_const("IMG_FILTER_COLORIZE", e_int(4))
}

/// `IMG_FILTER_EDGEDETECT` — transcribed from the PHP form.
fn decl_const_img_filter_edgedetect() -> Stmt {
    s_const("IMG_FILTER_EDGEDETECT", e_int(5))
}

/// `IMG_FILTER_EMBOSS` — transcribed from the PHP form.
fn decl_const_img_filter_emboss() -> Stmt {
    s_const("IMG_FILTER_EMBOSS", e_int(6))
}

/// `IMG_FILTER_GAUSSIAN_BLUR` — transcribed from the PHP form.
fn decl_const_img_filter_gaussian_blur() -> Stmt {
    s_const("IMG_FILTER_GAUSSIAN_BLUR", e_int(7))
}

/// `IMG_FILTER_SELECTIVE_BLUR` — transcribed from the PHP form.
fn decl_const_img_filter_selective_blur() -> Stmt {
    s_const("IMG_FILTER_SELECTIVE_BLUR", e_int(8))
}

/// `IMG_FILTER_MEAN_REMOVAL` — transcribed from the PHP form.
fn decl_const_img_filter_mean_removal() -> Stmt {
    s_const("IMG_FILTER_MEAN_REMOVAL", e_int(9))
}

/// `IMG_FILTER_SMOOTH` — transcribed from the PHP form.
fn decl_const_img_filter_smooth() -> Stmt {
    s_const("IMG_FILTER_SMOOTH", e_int(10))
}

/// `IMG_FILTER_PIXELATE` — transcribed from the PHP form.
fn decl_const_img_filter_pixelate() -> Stmt {
    s_const("IMG_FILTER_PIXELATE", e_int(11))
}

/// `IMG_FILTER_SCATTER` — transcribed from the PHP form.
fn decl_const_img_filter_scatter() -> Stmt {
    s_const("IMG_FILTER_SCATTER", e_int(12))
}

/// `IMG_AFFINE_TRANSLATE` — transcribed from the PHP form.
fn decl_const_img_affine_translate() -> Stmt {
    s_const("IMG_AFFINE_TRANSLATE", e_int(0))
}

/// `IMG_AFFINE_SCALE` — transcribed from the PHP form.
fn decl_const_img_affine_scale() -> Stmt {
    s_const("IMG_AFFINE_SCALE", e_int(1))
}

/// `IMG_AFFINE_ROTATE` — transcribed from the PHP form.
fn decl_const_img_affine_rotate() -> Stmt {
    s_const("IMG_AFFINE_ROTATE", e_int(2))
}

/// `IMG_AFFINE_SHEAR_HORIZONTAL` — transcribed from the PHP form.
fn decl_const_img_affine_shear_horizontal() -> Stmt {
    s_const("IMG_AFFINE_SHEAR_HORIZONTAL", e_int(3))
}

/// `IMG_AFFINE_SHEAR_VERTICAL` — transcribed from the PHP form.
fn decl_const_img_affine_shear_vertical() -> Stmt {
    s_const("IMG_AFFINE_SHEAR_VERTICAL", e_int(4))
}

/// `IMG_CROP_DEFAULT` — transcribed from the PHP form.
fn decl_const_img_crop_default() -> Stmt {
    s_const("IMG_CROP_DEFAULT", e_int(0))
}

/// `IMG_CROP_TRANSPARENT` — transcribed from the PHP form.
fn decl_const_img_crop_transparent() -> Stmt {
    s_const("IMG_CROP_TRANSPARENT", e_int(1))
}

/// `IMG_CROP_BLACK` — transcribed from the PHP form.
fn decl_const_img_crop_black() -> Stmt {
    s_const("IMG_CROP_BLACK", e_int(2))
}

/// `IMG_CROP_WHITE` — transcribed from the PHP form.
fn decl_const_img_crop_white() -> Stmt {
    s_const("IMG_CROP_WHITE", e_int(3))
}

/// `IMG_CROP_SIDES` — transcribed from the PHP form.
fn decl_const_img_crop_sides() -> Stmt {
    s_const("IMG_CROP_SIDES", e_int(4))
}

/// `IMG_CROP_THRESHOLD` — transcribed from the PHP form.
fn decl_const_img_crop_threshold() -> Stmt {
    s_const("IMG_CROP_THRESHOLD", e_int(5))
}

/// `IMG_BELL` — transcribed from the PHP form.
fn decl_const_img_bell() -> Stmt {
    s_const("IMG_BELL", e_int(1))
}

/// `IMG_BESSEL` — transcribed from the PHP form.
fn decl_const_img_bessel() -> Stmt {
    s_const("IMG_BESSEL", e_int(2))
}

/// `IMG_BILINEAR_FIXED` — transcribed from the PHP form.
fn decl_const_img_bilinear_fixed() -> Stmt {
    s_const("IMG_BILINEAR_FIXED", e_int(3))
}

/// `IMG_BICUBIC` — transcribed from the PHP form.
fn decl_const_img_bicubic() -> Stmt {
    s_const("IMG_BICUBIC", e_int(4))
}

/// `IMG_BICUBIC_FIXED` — transcribed from the PHP form.
fn decl_const_img_bicubic_fixed() -> Stmt {
    s_const("IMG_BICUBIC_FIXED", e_int(5))
}

/// `IMG_BLACKMAN` — transcribed from the PHP form.
fn decl_const_img_blackman() -> Stmt {
    s_const("IMG_BLACKMAN", e_int(6))
}

/// `IMG_BOX` — transcribed from the PHP form.
fn decl_const_img_box() -> Stmt {
    s_const("IMG_BOX", e_int(7))
}

/// `IMG_BSPLINE` — transcribed from the PHP form.
fn decl_const_img_bspline() -> Stmt {
    s_const("IMG_BSPLINE", e_int(8))
}

/// `IMG_CATMULLROM` — transcribed from the PHP form.
fn decl_const_img_catmullrom() -> Stmt {
    s_const("IMG_CATMULLROM", e_int(9))
}

/// `IMG_GAUSSIAN` — transcribed from the PHP form.
fn decl_const_img_gaussian() -> Stmt {
    s_const("IMG_GAUSSIAN", e_int(10))
}

/// `IMG_GENERALIZED_CUBIC` — transcribed from the PHP form.
fn decl_const_img_generalized_cubic() -> Stmt {
    s_const("IMG_GENERALIZED_CUBIC", e_int(11))
}

/// `IMG_HERMITE` — transcribed from the PHP form.
fn decl_const_img_hermite() -> Stmt {
    s_const("IMG_HERMITE", e_int(12))
}

/// `IMG_HAMMING` — transcribed from the PHP form.
fn decl_const_img_hamming() -> Stmt {
    s_const("IMG_HAMMING", e_int(13))
}

/// `IMG_HANNING` — transcribed from the PHP form.
fn decl_const_img_hanning() -> Stmt {
    s_const("IMG_HANNING", e_int(14))
}

/// `IMG_MITCHELL` — transcribed from the PHP form.
fn decl_const_img_mitchell() -> Stmt {
    s_const("IMG_MITCHELL", e_int(15))
}

/// `IMG_NEAREST_NEIGHBOUR` — transcribed from the PHP form.
fn decl_const_img_nearest_neighbour() -> Stmt {
    s_const("IMG_NEAREST_NEIGHBOUR", e_int(16))
}

/// `IMG_POWER` — transcribed from the PHP form.
fn decl_const_img_power() -> Stmt {
    s_const("IMG_POWER", e_int(17))
}

/// `IMG_QUADRATIC` — transcribed from the PHP form.
fn decl_const_img_quadratic() -> Stmt {
    s_const("IMG_QUADRATIC", e_int(18))
}

/// `IMG_SINC` — transcribed from the PHP form.
fn decl_const_img_sinc() -> Stmt {
    s_const("IMG_SINC", e_int(19))
}

/// `IMG_TRIANGLE` — transcribed from the PHP form.
fn decl_const_img_triangle() -> Stmt {
    s_const("IMG_TRIANGLE", e_int(20))
}

/// `IMG_WEIGHTED4` — transcribed from the PHP form.
fn decl_const_img_weighted4() -> Stmt {
    s_const("IMG_WEIGHTED4", e_int(21))
}

/// `GdImage` — transcribed from the PHP form.
fn decl_class_gdimage() -> Stmt {
    class("GdImage")
        .final_()
        .prop("handle", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param("handle", TypeExpr::Int)
                .body(vec![
                    s_prop_assign(e_this(), "handle", e_var("handle")),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_expr(e_call("elephc_img_destroy", vec![e_this_prop("handle")])),
                ]),
        )
        .build()
}

/// `imagecreatetruecolor` — transcribed from the PHP form.
fn decl_fn_imagecreatetruecolor() -> Stmt {
    function("imagecreatetruecolor")
        .param("width", TypeExpr::Int)
        .param("height", TypeExpr::Int)
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("handle", e_call("elephc_img_create_truecolor", vec![e_var("width"), e_var("height")])),
            s_return(e_new("GdImage", vec![e_var("handle")])),
        ])
        .build()
}

/// `imagecreate` — transcribed from the PHP form.
fn decl_fn_imagecreate() -> Stmt {
    function("imagecreate")
        .param("width", TypeExpr::Int)
        .param("height", TypeExpr::Int)
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("handle", e_call("elephc_img_create", vec![e_var("width"), e_var("height")])),
            s_return(e_new("GdImage", vec![e_var("handle")])),
        ])
        .build()
}

/// `imagecolorallocate` — transcribed from the PHP form.
fn decl_fn_imagecolorallocate() -> Stmt {
    function("imagecolorallocate")
        .param("image", t_class("GdImage"))
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_allocate", vec![e_prop(e_var("image"), "handle"), e_var("red"), e_var("green"), e_var("blue")])),
        ])
        .build()
}

/// `imagecolorallocatealpha` — transcribed from the PHP form.
fn decl_fn_imagecolorallocatealpha() -> Stmt {
    function("imagecolorallocatealpha")
        .param("image", t_class("GdImage"))
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .param("alpha", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_allocate_alpha", vec![e_prop(e_var("image"), "handle"), e_var("red"), e_var("green"), e_var("blue"), e_var("alpha")])),
        ])
        .build()
}

/// `imagesetpixel` — transcribed from the PHP form.
fn decl_fn_imagesetpixel() -> Stmt {
    function("imagesetpixel")
        .param("image", t_class("GdImage"))
        .param("x", TypeExpr::Int)
        .param("y", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_set_pixel", vec![e_prop(e_var("image"), "handle"), e_var("x"), e_var("y"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagesx` — transcribed from the PHP form.
fn decl_fn_imagesx() -> Stmt {
    function("imagesx")
        .param("image", t_class("GdImage"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_sx", vec![e_prop(e_var("image"), "handle")])),
        ])
        .build()
}

/// `imagesy` — transcribed from the PHP form.
fn decl_fn_imagesy() -> Stmt {
    function("imagesy")
        .param("image", t_class("GdImage"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_sy", vec![e_prop(e_var("image"), "handle")])),
        ])
        .build()
}

/// `imagedestroy` — transcribed from the PHP form.
fn decl_fn_imagedestroy() -> Stmt {
    function("imagedestroy")
        .param("image", t_class("GdImage"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_destroy", vec![e_prop(e_var("image"), "handle")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imageistruecolor` — transcribed from the PHP form.
fn decl_fn_imageistruecolor() -> Stmt {
    function("imageistruecolor")
        .param("image", t_class("GdImage"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_call("elephc_img_is_truecolor", vec![e_prop(e_var("image"), "handle")]), BinOp::StrictEq, e_int(1))),
        ])
        .build()
}

/// `imageresolution` — transcribed from the PHP form.
fn decl_fn_imageresolution() -> Stmt {
    function("imageresolution")
        .param("image", t_class("GdImage"))
        .param_default("resolution_x", t_nullable(TypeExpr::Int), e_null())
        .param_default("resolution_y", t_nullable(TypeExpr::Int), e_null())
        .returns(t_union(vec![t_array(), TypeExpr::Bool]))
        .body(vec![
            s_if(
                e_binop(e_var("resolution_x"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_array(vec![e_call("elephc_img_res_x", vec![e_prop(e_var("image"), "handle")]), e_call("elephc_img_res_y", vec![e_prop(e_var("image"), "handle")])])),
                ],
                vec![],
                None,
            ),
            s_assign("_ry", e_null_coalesce(e_var("resolution_y"), e_var("resolution_x"))),
            s_expr(e_call("elephc_img_set_res", vec![e_prop(e_var("image"), "handle"), e_cast(CastType::Int, e_var("resolution_x")), e_cast(CastType::Int, e_var("_ry"))])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagecolorat` — transcribed from the PHP form.
fn decl_fn_imagecolorat() -> Stmt {
    function("imagecolorat")
        .param("image", t_class("GdImage"))
        .param("x", TypeExpr::Int)
        .param("y", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_at", vec![e_prop(e_var("image"), "handle"), e_var("x"), e_var("y")])),
        ])
        .build()
}

/// `imagecolorsforindex` — transcribed from the PHP form.
fn decl_fn_imagecolorsforindex() -> Stmt {
    function("imagecolorsforindex")
        .param("image", t_class("GdImage"))
        .param("color", TypeExpr::Int)
        .returns(t_array())
        .body(vec![
            s_assign("_unused", e_var("image")),
            s_return(e_array_assoc(vec![(e_str("red"), e_binop(e_binop(e_var("color"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))), (e_str("green"), e_binop(e_binop(e_var("color"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))), (e_str("blue"), e_binop(e_var("color"), BinOp::BitAnd, e_int(255))), (e_str("alpha"), e_binop(e_binop(e_var("color"), BinOp::ShiftRight, e_int(24)), BinOp::BitAnd, e_int(127)))])),
        ])
        .build()
}

/// `imagecolordeallocate` — transcribed from the PHP form.
fn decl_fn_imagecolordeallocate() -> Stmt {
    function("imagecolordeallocate")
        .param("image", t_class("GdImage"))
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_unused", e_var("image")),
            s_assign("_color", e_var("color")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagecolorexact` — transcribed from the PHP form.
fn decl_fn_imagecolorexact() -> Stmt {
    function("imagecolorexact")
        .param("image", t_class("GdImage"))
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_allocate", vec![e_prop(e_var("image"), "handle"), e_var("red"), e_var("green"), e_var("blue")])),
        ])
        .build()
}

/// `imagecolorexactalpha` — transcribed from the PHP form.
fn decl_fn_imagecolorexactalpha() -> Stmt {
    function("imagecolorexactalpha")
        .param("image", t_class("GdImage"))
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .param("alpha", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_allocate_alpha", vec![e_prop(e_var("image"), "handle"), e_var("red"), e_var("green"), e_var("blue"), e_var("alpha")])),
        ])
        .build()
}

/// `imagecolorclosest` — transcribed from the PHP form.
fn decl_fn_imagecolorclosest() -> Stmt {
    function("imagecolorclosest")
        .param("image", t_class("GdImage"))
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_allocate", vec![e_prop(e_var("image"), "handle"), e_var("red"), e_var("green"), e_var("blue")])),
        ])
        .build()
}

/// `imagecolorclosestalpha` — transcribed from the PHP form.
fn decl_fn_imagecolorclosestalpha() -> Stmt {
    function("imagecolorclosestalpha")
        .param("image", t_class("GdImage"))
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .param("alpha", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_allocate_alpha", vec![e_prop(e_var("image"), "handle"), e_var("red"), e_var("green"), e_var("blue"), e_var("alpha")])),
        ])
        .build()
}

/// `imagecolorclosesthwb` — transcribed from the PHP form.
fn decl_fn_imagecolorclosesthwb() -> Stmt {
    function("imagecolorclosesthwb")
        .param("image", t_class("GdImage"))
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_allocate", vec![e_prop(e_var("image"), "handle"), e_var("red"), e_var("green"), e_var("blue")])),
        ])
        .build()
}

/// `imagecolorresolve` — transcribed from the PHP form.
fn decl_fn_imagecolorresolve() -> Stmt {
    function("imagecolorresolve")
        .param("image", t_class("GdImage"))
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_allocate", vec![e_prop(e_var("image"), "handle"), e_var("red"), e_var("green"), e_var("blue")])),
        ])
        .build()
}

/// `imagecolorresolvealpha` — transcribed from the PHP form.
fn decl_fn_imagecolorresolvealpha() -> Stmt {
    function("imagecolorresolvealpha")
        .param("image", t_class("GdImage"))
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .param("alpha", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_allocate_alpha", vec![e_prop(e_var("image"), "handle"), e_var("red"), e_var("green"), e_var("blue"), e_var("alpha")])),
        ])
        .build()
}

/// `imagecolortransparent` — transcribed from the PHP form.
fn decl_fn_imagecolortransparent() -> Stmt {
    function("imagecolortransparent")
        .param("image", t_class("GdImage"))
        .param_default("color", t_nullable(TypeExpr::Int), e_null())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_var("color"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_call("elephc_img_get_transparent", vec![e_prop(e_var("image"), "handle")])),
                ],
                vec![],
                None,
            ),
            s_assign("_c", e_cast(CastType::Int, e_var("color"))),
            s_expr(e_call("elephc_img_set_transparent", vec![e_prop(e_var("image"), "handle"), e_var("_c")])),
            s_return(e_var("_c")),
        ])
        .build()
}

/// `imagecolorstotal` — transcribed from the PHP form.
fn decl_fn_imagecolorstotal() -> Stmt {
    function("imagecolorstotal")
        .param("image", t_class("GdImage"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_color_total", vec![e_prop(e_var("image"), "handle")])),
        ])
        .build()
}

/// `imagealphablending` — transcribed from the PHP form.
fn decl_fn_imagealphablending() -> Stmt {
    function("imagealphablending")
        .param("image", t_class("GdImage"))
        .param("enable", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_set_alpha_blending", vec![e_prop(e_var("image"), "handle"), e_ternary(e_var("enable"), e_int(1), e_int(0))])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagesavealpha` — transcribed from the PHP form.
fn decl_fn_imagesavealpha() -> Stmt {
    function("imagesavealpha")
        .param("image", t_class("GdImage"))
        .param("enable", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_set_save_alpha", vec![e_prop(e_var("image"), "handle"), e_ternary(e_var("enable"), e_int(1), e_int(0))])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagepalettetotruecolor` — transcribed from the PHP form.
fn decl_fn_imagepalettetotruecolor() -> Stmt {
    function("imagepalettetotruecolor")
        .param("image", t_class("GdImage"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_set_truecolor", vec![e_prop(e_var("image"), "handle"), e_int(1)])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagetruecolortopalette` — transcribed from the PHP form.
fn decl_fn_imagetruecolortopalette() -> Stmt {
    function("imagetruecolortopalette")
        .param("image", t_class("GdImage"))
        .param("dither", TypeExpr::Bool)
        .param("num_colors", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_unused", e_var("dither")),
            s_assign("_n", e_var("num_colors")),
            s_expr(e_call("elephc_img_set_truecolor", vec![e_prop(e_var("image"), "handle"), e_int(0)])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagecolormatch` — transcribed from the PHP form.
fn decl_fn_imagecolormatch() -> Stmt {
    function("imagecolormatch")
        .param("image1", t_class("GdImage"))
        .param("image2", t_class("GdImage"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_u1", e_var("image1")),
            s_assign("_u2", e_var("image2")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagecolorset` — transcribed from the PHP form.
fn decl_fn_imagecolorset() -> Stmt {
    function("imagecolorset")
        .param("image", t_class("GdImage"))
        .param("color", TypeExpr::Int)
        .param("red", TypeExpr::Int)
        .param("green", TypeExpr::Int)
        .param("blue", TypeExpr::Int)
        .param_default("alpha", TypeExpr::Int, e_int(0))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_image", e_var("image")),
            s_assign("_color", e_var("color")),
            s_assign("_red", e_var("red")),
            s_assign("_green", e_var("green")),
            s_assign("_blue", e_var("blue")),
            s_assign("_alpha", e_var("alpha")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagepalettecopy` — transcribed from the PHP form.
fn decl_fn_imagepalettecopy() -> Stmt {
    function("imagepalettecopy")
        .param("dst", t_class("GdImage"))
        .param("src", t_class("GdImage"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_dst", e_var("dst")),
            s_assign("_src", e_var("src")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagelayereffect` — transcribed from the PHP form.
fn decl_fn_imagelayereffect() -> Stmt {
    function("imagelayereffect")
        .param("image", t_class("GdImage"))
        .param("effect", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("effect"), BinOp::StrictEq, e_const("IMG_EFFECT_REPLACE")),
                vec![
                    s_expr(e_call("elephc_img_set_alpha_blending", vec![e_prop(e_var("image"), "handle"), e_int(0)])),
                ],
                vec![],
                Some(vec![
                s_expr(e_call("elephc_img_set_alpha_blending", vec![e_prop(e_var("image"), "handle"), e_int(1)])),
            ]),
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagesetthickness` — transcribed from the PHP form.
fn decl_fn_imagesetthickness() -> Stmt {
    function("imagesetthickness")
        .param("image", t_class("GdImage"))
        .param("thickness", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_set_thickness", vec![e_prop(e_var("image"), "handle"), e_var("thickness")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imageline` — transcribed from the PHP form.
fn decl_fn_imageline() -> Stmt {
    function("imageline")
        .param("image", t_class("GdImage"))
        .param("x1", TypeExpr::Int)
        .param("y1", TypeExpr::Int)
        .param("x2", TypeExpr::Int)
        .param("y2", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_line", vec![e_prop(e_var("image"), "handle"), e_var("x1"), e_var("y1"), e_var("x2"), e_var("y2"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagedashedline` — transcribed from the PHP form.
fn decl_fn_imagedashedline() -> Stmt {
    function("imagedashedline")
        .param("image", t_class("GdImage"))
        .param("x1", TypeExpr::Int)
        .param("y1", TypeExpr::Int)
        .param("x2", TypeExpr::Int)
        .param("y2", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_dashed_line", vec![e_prop(e_var("image"), "handle"), e_var("x1"), e_var("y1"), e_var("x2"), e_var("y2"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagerectangle` — transcribed from the PHP form.
fn decl_fn_imagerectangle() -> Stmt {
    function("imagerectangle")
        .param("image", t_class("GdImage"))
        .param("x1", TypeExpr::Int)
        .param("y1", TypeExpr::Int)
        .param("x2", TypeExpr::Int)
        .param("y2", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_rectangle", vec![e_prop(e_var("image"), "handle"), e_var("x1"), e_var("y1"), e_var("x2"), e_var("y2"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagefilledrectangle` — transcribed from the PHP form.
fn decl_fn_imagefilledrectangle() -> Stmt {
    function("imagefilledrectangle")
        .param("image", t_class("GdImage"))
        .param("x1", TypeExpr::Int)
        .param("y1", TypeExpr::Int)
        .param("x2", TypeExpr::Int)
        .param("y2", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_filled_rectangle", vec![e_prop(e_var("image"), "handle"), e_var("x1"), e_var("y1"), e_var("x2"), e_var("y2"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imageellipse` — transcribed from the PHP form.
fn decl_fn_imageellipse() -> Stmt {
    function("imageellipse")
        .param("image", t_class("GdImage"))
        .param("center_x", TypeExpr::Int)
        .param("center_y", TypeExpr::Int)
        .param("width", TypeExpr::Int)
        .param("height", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_ellipse", vec![e_prop(e_var("image"), "handle"), e_var("center_x"), e_var("center_y"), e_var("width"), e_var("height"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagefilledellipse` — transcribed from the PHP form.
fn decl_fn_imagefilledellipse() -> Stmt {
    function("imagefilledellipse")
        .param("image", t_class("GdImage"))
        .param("center_x", TypeExpr::Int)
        .param("center_y", TypeExpr::Int)
        .param("width", TypeExpr::Int)
        .param("height", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_filled_ellipse", vec![e_prop(e_var("image"), "handle"), e_var("center_x"), e_var("center_y"), e_var("width"), e_var("height"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagearc` — transcribed from the PHP form.
fn decl_fn_imagearc() -> Stmt {
    function("imagearc")
        .param("image", t_class("GdImage"))
        .param("center_x", TypeExpr::Int)
        .param("center_y", TypeExpr::Int)
        .param("width", TypeExpr::Int)
        .param("height", TypeExpr::Int)
        .param("start_angle", TypeExpr::Int)
        .param("end_angle", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_cxy", e_binop(e_binop(e_var("center_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("center_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_wh", e_binop(e_binop(e_var("width"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("height"), BinOp::BitAnd, e_int(4294967295)))),
            s_expr(e_call("elephc_img_arc", vec![e_prop(e_var("image"), "handle"), e_var("_cxy"), e_var("_wh"), e_var("start_angle"), e_var("end_angle"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagefilledarc` — transcribed from the PHP form.
fn decl_fn_imagefilledarc() -> Stmt {
    function("imagefilledarc")
        .param("image", t_class("GdImage"))
        .param("center_x", TypeExpr::Int)
        .param("center_y", TypeExpr::Int)
        .param("width", TypeExpr::Int)
        .param("height", TypeExpr::Int)
        .param("start_angle", TypeExpr::Int)
        .param("end_angle", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .param("style", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_cxy", e_binop(e_binop(e_var("center_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("center_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_wh", e_binop(e_binop(e_var("width"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("height"), BinOp::BitAnd, e_int(4294967295)))),
            s_if(
                e_binop(e_binop(e_var("style"), BinOp::BitAnd, e_const("IMG_ARC_NOFILL")), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_expr(e_call("elephc_img_arc", vec![e_prop(e_var("image"), "handle"), e_var("_cxy"), e_var("_wh"), e_var("start_angle"), e_var("end_angle"), e_var("color")])),
                ],
                vec![],
                Some(vec![
                s_expr(e_call("elephc_img_filled_arc", vec![e_prop(e_var("image"), "handle"), e_var("_cxy"), e_var("_wh"), e_var("start_angle"), e_var("end_angle"), e_var("color")])),
            ]),
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagefill` — transcribed from the PHP form.
fn decl_fn_imagefill() -> Stmt {
    function("imagefill")
        .param("image", t_class("GdImage"))
        .param("x", TypeExpr::Int)
        .param("y", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_fill", vec![e_prop(e_var("image"), "handle"), e_var("x"), e_var("y"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagefilltoborder` — transcribed from the PHP form.
fn decl_fn_imagefilltoborder() -> Stmt {
    function("imagefilltoborder")
        .param("image", t_class("GdImage"))
        .param("x", TypeExpr::Int)
        .param("y", TypeExpr::Int)
        .param("border_color", TypeExpr::Int)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_fill_to_border", vec![e_prop(e_var("image"), "handle"), e_var("x"), e_var("y"), e_var("border_color"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagepolygon` — transcribed from the PHP form.
fn decl_fn_imagepolygon() -> Stmt {
    function("imagepolygon")
        .param("image", t_class("GdImage"))
        .param("points", t_array())
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_poly_reset", vec![])),
            s_assign("_n", e_call("count", vec![e_var("points")])),
            s_assign("_i", e_int(0)),
            s_while(e_binop(e_binop(e_var("_i"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("_n")), vec![
                s_expr(e_call("elephc_img_poly_add", vec![e_cast(CastType::Int, e_index(e_var("points"), e_var("_i"))), e_cast(CastType::Int, e_index(e_var("points"), e_binop(e_var("_i"), BinOp::Add, e_int(1))))])),
                s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(2))),
            ]),
            s_expr(e_call("elephc_img_poly_line", vec![e_prop(e_var("image"), "handle"), e_var("color"), e_int(1)])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imageopenpolygon` — transcribed from the PHP form.
fn decl_fn_imageopenpolygon() -> Stmt {
    function("imageopenpolygon")
        .param("image", t_class("GdImage"))
        .param("points", t_array())
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_poly_reset", vec![])),
            s_assign("_n", e_call("count", vec![e_var("points")])),
            s_assign("_i", e_int(0)),
            s_while(e_binop(e_binop(e_var("_i"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("_n")), vec![
                s_expr(e_call("elephc_img_poly_add", vec![e_cast(CastType::Int, e_index(e_var("points"), e_var("_i"))), e_cast(CastType::Int, e_index(e_var("points"), e_binop(e_var("_i"), BinOp::Add, e_int(1))))])),
                s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(2))),
            ]),
            s_expr(e_call("elephc_img_poly_line", vec![e_prop(e_var("image"), "handle"), e_var("color"), e_int(0)])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagefilledpolygon` — transcribed from the PHP form.
fn decl_fn_imagefilledpolygon() -> Stmt {
    function("imagefilledpolygon")
        .param("image", t_class("GdImage"))
        .param("points", t_array())
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_poly_reset", vec![])),
            s_assign("_n", e_call("count", vec![e_var("points")])),
            s_assign("_i", e_int(0)),
            s_while(e_binop(e_binop(e_var("_i"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("_n")), vec![
                s_expr(e_call("elephc_img_poly_add", vec![e_cast(CastType::Int, e_index(e_var("points"), e_var("_i"))), e_cast(CastType::Int, e_index(e_var("points"), e_binop(e_var("_i"), BinOp::Add, e_int(1))))])),
                s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(2))),
            ]),
            s_expr(e_call("elephc_img_poly_fill", vec![e_prop(e_var("image"), "handle"), e_var("color")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagestring` — transcribed from the PHP form.
fn decl_fn_imagestring() -> Stmt {
    function("imagestring")
        .param("image", t_class("GdImage"))
        .param("font", TypeExpr::Int)
        .param("x", TypeExpr::Int)
        .param("y", TypeExpr::Int)
        .param("string", TypeExpr::Str)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_string", vec![e_prop(e_var("image"), "handle"), e_var("font"), e_var("x"), e_var("y"), e_var("color"), e_var("string")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagestringup` — transcribed from the PHP form.
fn decl_fn_imagestringup() -> Stmt {
    function("imagestringup")
        .param("image", t_class("GdImage"))
        .param("font", TypeExpr::Int)
        .param("x", TypeExpr::Int)
        .param("y", TypeExpr::Int)
        .param("string", TypeExpr::Str)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_string_up", vec![e_prop(e_var("image"), "handle"), e_var("font"), e_var("x"), e_var("y"), e_var("color"), e_var("string")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagechar` — transcribed from the PHP form.
fn decl_fn_imagechar() -> Stmt {
    function("imagechar")
        .param("image", t_class("GdImage"))
        .param("font", TypeExpr::Int)
        .param("x", TypeExpr::Int)
        .param("y", TypeExpr::Int)
        .param("char", TypeExpr::Str)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_string", vec![e_prop(e_var("image"), "handle"), e_var("font"), e_var("x"), e_var("y"), e_var("color"), e_call("substr", vec![e_var("char"), e_int(0), e_int(1)])])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagecharup` — transcribed from the PHP form.
fn decl_fn_imagecharup() -> Stmt {
    function("imagecharup")
        .param("image", t_class("GdImage"))
        .param("font", TypeExpr::Int)
        .param("x", TypeExpr::Int)
        .param("y", TypeExpr::Int)
        .param("char", TypeExpr::Str)
        .param("color", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_string_up", vec![e_prop(e_var("image"), "handle"), e_var("font"), e_var("x"), e_var("y"), e_var("color"), e_call("substr", vec![e_var("char"), e_int(0), e_int(1)])])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagefontwidth` — transcribed from the PHP form.
fn decl_fn_imagefontwidth() -> Stmt {
    function("imagefontwidth")
        .param("font", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_unused", e_var("font")),
            s_return(e_int(8)),
        ])
        .build()
}

/// `imagefontheight` — transcribed from the PHP form.
fn decl_fn_imagefontheight() -> Stmt {
    function("imagefontheight")
        .param("font", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_unused", e_var("font")),
            s_return(e_int(8)),
        ])
        .build()
}

/// `imagecopy` — transcribed from the PHP form.
fn decl_fn_imagecopy() -> Stmt {
    function("imagecopy")
        .param("dst_image", t_class("GdImage"))
        .param("src_image", t_class("GdImage"))
        .param("dst_x", TypeExpr::Int)
        .param("dst_y", TypeExpr::Int)
        .param("src_x", TypeExpr::Int)
        .param("src_y", TypeExpr::Int)
        .param("src_width", TypeExpr::Int)
        .param("src_height", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_dxy", e_binop(e_binop(e_var("dst_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("dst_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_sxy", e_binop(e_binop(e_var("src_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_swh", e_binop(e_binop(e_var("src_width"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_height"), BinOp::BitAnd, e_int(4294967295)))),
            s_return(e_binop(e_call("elephc_img_copy", vec![e_prop(e_var("dst_image"), "handle"), e_prop(e_var("src_image"), "handle"), e_var("_dxy"), e_var("_sxy"), e_var("_swh")]), BinOp::StrictEq, e_int(0))),
        ])
        .build()
}

/// `imagecopymerge` — transcribed from the PHP form.
fn decl_fn_imagecopymerge() -> Stmt {
    function("imagecopymerge")
        .param("dst_image", t_class("GdImage"))
        .param("src_image", t_class("GdImage"))
        .param("dst_x", TypeExpr::Int)
        .param("dst_y", TypeExpr::Int)
        .param("src_x", TypeExpr::Int)
        .param("src_y", TypeExpr::Int)
        .param("src_width", TypeExpr::Int)
        .param("src_height", TypeExpr::Int)
        .param("pct", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_dxy", e_binop(e_binop(e_var("dst_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("dst_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_sxy", e_binop(e_binop(e_var("src_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_swh", e_binop(e_binop(e_var("src_width"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_height"), BinOp::BitAnd, e_int(4294967295)))),
            s_return(e_binop(e_call("elephc_img_copy_merge", vec![e_prop(e_var("dst_image"), "handle"), e_prop(e_var("src_image"), "handle"), e_var("_dxy"), e_var("_sxy"), e_var("_swh"), e_var("pct")]), BinOp::StrictEq, e_int(0))),
        ])
        .build()
}

/// `imagecopymergegray` — transcribed from the PHP form.
fn decl_fn_imagecopymergegray() -> Stmt {
    function("imagecopymergegray")
        .param("dst_image", t_class("GdImage"))
        .param("src_image", t_class("GdImage"))
        .param("dst_x", TypeExpr::Int)
        .param("dst_y", TypeExpr::Int)
        .param("src_x", TypeExpr::Int)
        .param("src_y", TypeExpr::Int)
        .param("src_width", TypeExpr::Int)
        .param("src_height", TypeExpr::Int)
        .param("pct", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_dxy", e_binop(e_binop(e_var("dst_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("dst_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_sxy", e_binop(e_binop(e_var("src_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_swh", e_binop(e_binop(e_var("src_width"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_height"), BinOp::BitAnd, e_int(4294967295)))),
            s_return(e_binop(e_call("elephc_img_copy_merge_gray", vec![e_prop(e_var("dst_image"), "handle"), e_prop(e_var("src_image"), "handle"), e_var("_dxy"), e_var("_sxy"), e_var("_swh"), e_var("pct")]), BinOp::StrictEq, e_int(0))),
        ])
        .build()
}

/// `imagecopyresized` — transcribed from the PHP form.
fn decl_fn_imagecopyresized() -> Stmt {
    function("imagecopyresized")
        .param("dst_image", t_class("GdImage"))
        .param("src_image", t_class("GdImage"))
        .param("dst_x", TypeExpr::Int)
        .param("dst_y", TypeExpr::Int)
        .param("src_x", TypeExpr::Int)
        .param("src_y", TypeExpr::Int)
        .param("dst_width", TypeExpr::Int)
        .param("dst_height", TypeExpr::Int)
        .param("src_width", TypeExpr::Int)
        .param("src_height", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_dxy", e_binop(e_binop(e_var("dst_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("dst_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_sxy", e_binop(e_binop(e_var("src_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_dwh", e_binop(e_binop(e_var("dst_width"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("dst_height"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_swh", e_binop(e_binop(e_var("src_width"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_height"), BinOp::BitAnd, e_int(4294967295)))),
            s_return(e_binop(e_call("elephc_img_copy_resized", vec![e_prop(e_var("dst_image"), "handle"), e_prop(e_var("src_image"), "handle"), e_var("_dxy"), e_var("_sxy"), e_var("_dwh"), e_var("_swh")]), BinOp::StrictEq, e_int(0))),
        ])
        .build()
}

/// `imagecopyresampled` — transcribed from the PHP form.
fn decl_fn_imagecopyresampled() -> Stmt {
    function("imagecopyresampled")
        .param("dst_image", t_class("GdImage"))
        .param("src_image", t_class("GdImage"))
        .param("dst_x", TypeExpr::Int)
        .param("dst_y", TypeExpr::Int)
        .param("src_x", TypeExpr::Int)
        .param("src_y", TypeExpr::Int)
        .param("dst_width", TypeExpr::Int)
        .param("dst_height", TypeExpr::Int)
        .param("src_width", TypeExpr::Int)
        .param("src_height", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_dxy", e_binop(e_binop(e_var("dst_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("dst_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_sxy", e_binop(e_binop(e_var("src_x"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_y"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_dwh", e_binop(e_binop(e_var("dst_width"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("dst_height"), BinOp::BitAnd, e_int(4294967295)))),
            s_assign("_swh", e_binop(e_binop(e_var("src_width"), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("src_height"), BinOp::BitAnd, e_int(4294967295)))),
            s_return(e_binop(e_call("elephc_img_copy_resampled", vec![e_prop(e_var("dst_image"), "handle"), e_prop(e_var("src_image"), "handle"), e_var("_dxy"), e_var("_sxy"), e_var("_dwh"), e_var("_swh")]), BinOp::StrictEq, e_int(0))),
        ])
        .build()
}

/// `imagescale` — transcribed from the PHP form.
fn decl_fn_imagescale() -> Stmt {
    function("imagescale")
        .param("image", t_class("GdImage"))
        .param("width", TypeExpr::Int)
        .param_default("height", TypeExpr::Int, e_neg(e_int(1)))
        .param_default("mode", TypeExpr::Int, e_const("IMG_BILINEAR_FIXED"))
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_handle", e_call("elephc_img_scale", vec![e_prop(e_var("image"), "handle"), e_var("width"), e_var("height"), e_var("mode")])),
            s_if(
                e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_str("imagescale(): invalid target dimensions")])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_handle")])),
        ])
        .build()
}

/// `imagecrop` — transcribed from the PHP form.
fn decl_fn_imagecrop() -> Stmt {
    function("imagecrop")
        .param("image", t_class("GdImage"))
        .param_untyped_default("rect", e_array_assoc(vec![(e_str("x"), e_int(0)), (e_str("y"), e_int(0)), (e_str("width"), e_int(0)), (e_str("height"), e_int(0))]))
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_x", e_cast(CastType::Int, e_index(e_var("rect"), e_str("x")))),
            s_assign("_y", e_cast(CastType::Int, e_index(e_var("rect"), e_str("y")))),
            s_assign("_w", e_cast(CastType::Int, e_index(e_var("rect"), e_str("width")))),
            s_assign("_h", e_cast(CastType::Int, e_index(e_var("rect"), e_str("height")))),
            s_assign("_handle", e_call("elephc_img_crop", vec![e_prop(e_var("image"), "handle"), e_var("_x"), e_var("_y"), e_var("_w"), e_var("_h")])),
            s_if(
                e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_str("imagecrop(): invalid crop rectangle")])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_handle")])),
        ])
        .build()
}

/// `imagecropauto` — transcribed from the PHP form.
fn decl_fn_imagecropauto() -> Stmt {
    function("imagecropauto")
        .param("image", t_class("GdImage"))
        .param_default("mode", TypeExpr::Int, e_const("IMG_CROP_DEFAULT"))
        .param_default("threshold", TypeExpr::Float, e_float(0.5))
        .param_default("color", TypeExpr::Int, e_neg(e_int(1)))
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_t", e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("threshold"), BinOp::Mul, e_int(1000))]))),
            s_assign("_handle", e_call("elephc_img_crop_auto", vec![e_prop(e_var("image"), "handle"), e_var("mode"), e_var("color"), e_var("_t")])),
            s_if(
                e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_str("imagecropauto(): nothing to crop or invalid image")])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_handle")])),
        ])
        .build()
}

/// `imageflip` — transcribed from the PHP form.
fn decl_fn_imageflip() -> Stmt {
    function("imageflip")
        .param("image", t_class("GdImage"))
        .param("mode", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_call("elephc_img_flip", vec![e_prop(e_var("image"), "handle"), e_var("mode")]), BinOp::StrictEq, e_int(0))),
        ])
        .build()
}

/// `imagerotate` — transcribed from the PHP form.
fn decl_fn_imagerotate() -> Stmt {
    function("imagerotate")
        .param("image", t_class("GdImage"))
        .param("angle", TypeExpr::Float)
        .param("background_color", TypeExpr::Int)
        .param_default("ignore_transparent", TypeExpr::Int, e_int(0))
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_unused", e_var("ignore_transparent")),
            s_assign("_mdeg", e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("angle"), BinOp::Mul, e_int(1000))]))),
            s_assign("_handle", e_call("elephc_img_rotate", vec![e_prop(e_var("image"), "handle"), e_var("_mdeg"), e_var("background_color")])),
            s_if(
                e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_str("imagerotate(): invalid image")])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_handle")])),
        ])
        .build()
}

/// `imageaffine` — transcribed from the PHP form.
fn decl_fn_imageaffine() -> Stmt {
    function("imageaffine")
        .param("image", t_class("GdImage"))
        .param("affine", t_array())
        .param_default("clip", t_nullable(t_array()), e_null())
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_unused", e_var("clip")),
            s_expr(e_call("elephc_img_fbuf_reset", vec![])),
            s_assign("_i", e_int(0)),
            s_while(e_binop(e_var("_i"), BinOp::Lt, e_int(6)), vec![
                s_expr(e_call("elephc_img_fbuf_push", vec![e_cast(CastType::Int, e_call("round", vec![e_binop(e_cast(CastType::Float, e_index(e_var("affine"), e_var("_i"))), BinOp::Mul, e_int(65536))]))])),
                s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(1))),
            ]),
            s_assign("_handle", e_call("elephc_img_affine", vec![e_prop(e_var("image"), "handle")])),
            s_if(
                e_binop(e_var("_handle"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_str("imageaffine(): invalid or singular affine matrix")])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_handle")])),
        ])
        .build()
}

/// `imageaffinematrixconcat` — transcribed from the PHP form.
fn decl_fn_imageaffinematrixconcat() -> Stmt {
    function("imageaffinematrixconcat")
        .param("matrix1", t_array())
        .param("matrix2", t_array())
        .returns(t_array())
        .body(vec![
            s_assign("_a1", e_cast(CastType::Float, e_index(e_var("matrix1"), e_int(0)))),
            s_assign("_b1", e_cast(CastType::Float, e_index(e_var("matrix1"), e_int(1)))),
            s_assign("_c1", e_cast(CastType::Float, e_index(e_var("matrix1"), e_int(2)))),
            s_assign("_d1", e_cast(CastType::Float, e_index(e_var("matrix1"), e_int(3)))),
            s_assign("_e1", e_cast(CastType::Float, e_index(e_var("matrix1"), e_int(4)))),
            s_assign("_f1", e_cast(CastType::Float, e_index(e_var("matrix1"), e_int(5)))),
            s_assign("_a2", e_cast(CastType::Float, e_index(e_var("matrix2"), e_int(0)))),
            s_assign("_b2", e_cast(CastType::Float, e_index(e_var("matrix2"), e_int(1)))),
            s_assign("_c2", e_cast(CastType::Float, e_index(e_var("matrix2"), e_int(2)))),
            s_assign("_d2", e_cast(CastType::Float, e_index(e_var("matrix2"), e_int(3)))),
            s_assign("_e2", e_cast(CastType::Float, e_index(e_var("matrix2"), e_int(4)))),
            s_assign("_f2", e_cast(CastType::Float, e_index(e_var("matrix2"), e_int(5)))),
            s_return(e_array(vec![e_binop(e_binop(e_var("_a1"), BinOp::Mul, e_var("_a2")), BinOp::Add, e_binop(e_var("_b1"), BinOp::Mul, e_var("_c2"))), e_binop(e_binop(e_var("_a1"), BinOp::Mul, e_var("_b2")), BinOp::Add, e_binop(e_var("_b1"), BinOp::Mul, e_var("_d2"))), e_binop(e_binop(e_var("_c1"), BinOp::Mul, e_var("_a2")), BinOp::Add, e_binop(e_var("_d1"), BinOp::Mul, e_var("_c2"))), e_binop(e_binop(e_var("_c1"), BinOp::Mul, e_var("_b2")), BinOp::Add, e_binop(e_var("_d1"), BinOp::Mul, e_var("_d2"))), e_binop(e_binop(e_binop(e_var("_e1"), BinOp::Mul, e_var("_a2")), BinOp::Add, e_binop(e_var("_f1"), BinOp::Mul, e_var("_c2"))), BinOp::Add, e_var("_e2")), e_binop(e_binop(e_binop(e_var("_e1"), BinOp::Mul, e_var("_b2")), BinOp::Add, e_binop(e_var("_f1"), BinOp::Mul, e_var("_d2"))), BinOp::Add, e_var("_f2"))])),
        ])
        .build()
}

/// `imagefilter` — transcribed from the PHP form.
fn decl_fn_imagefilter() -> Stmt {
    function("imagefilter")
        .param("image", t_class("GdImage"))
        .param("filter", TypeExpr::Int)
        .param_default("arg1", TypeExpr::Int, e_int(0))
        .param_default("arg2", TypeExpr::Int, e_int(0))
        .param_default("arg3", TypeExpr::Int, e_int(0))
        .param_default("arg4", TypeExpr::Int, e_int(0))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_call("elephc_img_filter", vec![e_prop(e_var("image"), "handle"), e_var("filter"), e_var("arg1"), e_var("arg2"), e_var("arg3"), e_var("arg4")]), BinOp::StrictEq, e_int(0))),
        ])
        .build()
}

/// `imageconvolution` — transcribed from the PHP form.
fn decl_fn_imageconvolution() -> Stmt {
    function("imageconvolution")
        .param("image", t_class("GdImage"))
        .param("matrix", t_array())
        .param("divisor", TypeExpr::Float)
        .param("offset", TypeExpr::Float)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_fbuf_reset", vec![])),
            s_assign("_r", e_int(0)),
            s_while(e_binop(e_var("_r"), BinOp::Lt, e_int(3)), vec![
                s_assign("_c", e_int(0)),
                s_while(e_binop(e_var("_c"), BinOp::Lt, e_int(3)), vec![
                    s_expr(e_call("elephc_img_fbuf_push", vec![e_cast(CastType::Int, e_call("round", vec![e_binop(e_cast(CastType::Float, e_index(e_index(e_var("matrix"), e_var("_r")), e_var("_c"))), BinOp::Mul, e_int(65536))]))])),
                    s_assign("_c", e_binop(e_var("_c"), BinOp::Add, e_int(1))),
                ]),
                s_assign("_r", e_binop(e_var("_r"), BinOp::Add, e_int(1))),
            ]),
            s_assign("_div", e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("divisor"), BinOp::Mul, e_int(65536))]))),
            s_assign("_off", e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("offset"), BinOp::Mul, e_int(65536))]))),
            s_return(e_binop(e_call("elephc_img_convolution", vec![e_prop(e_var("image"), "handle"), e_var("_div"), e_var("_off")]), BinOp::StrictEq, e_int(0))),
        ])
        .build()
}

/// `imagegammacorrect` — transcribed from the PHP form.
fn decl_fn_imagegammacorrect() -> Stmt {
    function("imagegammacorrect")
        .param("image", t_class("GdImage"))
        .param("input_gamma", TypeExpr::Float)
        .param("output_gamma", TypeExpr::Float)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_in", e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("input_gamma"), BinOp::Mul, e_int(65536))]))),
            s_assign("_out", e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("output_gamma"), BinOp::Mul, e_int(65536))]))),
            s_return(e_binop(e_call("elephc_img_gamma", vec![e_prop(e_var("image"), "handle"), e_var("_in"), e_var("_out")]), BinOp::StrictEq, e_int(0))),
        ])
        .build()
}

/// `imagesetinterpolation` — transcribed from the PHP form.
fn decl_fn_imagesetinterpolation() -> Stmt {
    function("imagesetinterpolation")
        .param("image", t_class("GdImage"))
        .param_default("method", TypeExpr::Int, e_const("IMG_BILINEAR_FIXED"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("elephc_img_set_interpolation", vec![e_prop(e_var("image"), "handle"), e_var("method")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagegetinterpolation` — transcribed from the PHP form.
fn decl_fn_imagegetinterpolation() -> Stmt {
    function("imagegetinterpolation")
        .param("image", t_class("GdImage"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_img_get_interpolation", vec![e_prop(e_var("image"), "handle")])),
        ])
        .build()
}

/// `imageinterlace` — transcribed from the PHP form.
fn decl_fn_imageinterlace() -> Stmt {
    function("imageinterlace")
        .param("image", t_class("GdImage"))
        .param_default("enable", t_nullable(TypeExpr::Bool), e_null())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_var("enable"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_img_set_interlace", vec![e_prop(e_var("image"), "handle"), e_ternary(e_var("enable"), e_int(1), e_int(0))])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_img_get_interlace", vec![e_prop(e_var("image"), "handle")])),
        ])
        .build()
}

/// `imageantialias` — transcribed from the PHP form.
fn decl_fn_imageantialias() -> Stmt {
    function("imageantialias")
        .param("image", t_class("GdImage"))
        .param("enable", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_unused", e_var("image")),
            s_assign("_u2", e_var("enable")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `ImageException` — transcribed from the PHP form.
fn decl_class_imageexception() -> Stmt {
    class("ImageException")
        .extends("RuntimeException")
        .build()
}

/// `imagecreatefrompng` — transcribed from the PHP form.
fn decl_fn_imagecreatefrompng() -> Stmt {
    function("imagecreatefrompng")
        .param("filename", TypeExpr::Str)
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_h", e_call("elephc_img_create_from_file", vec![e_var("filename"), e_int(1)])),
            s_if(
                e_binop(e_var("_h"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_binop(e_binop(e_str("imagecreatefrompng(): failed to open or decode '"), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("'"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_h")])),
        ])
        .build()
}

/// `imagecreatefromjpeg` — transcribed from the PHP form.
fn decl_fn_imagecreatefromjpeg() -> Stmt {
    function("imagecreatefromjpeg")
        .param("filename", TypeExpr::Str)
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_h", e_call("elephc_img_create_from_file", vec![e_var("filename"), e_int(2)])),
            s_if(
                e_binop(e_var("_h"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_binop(e_binop(e_str("imagecreatefromjpeg(): failed to open or decode '"), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("'"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_h")])),
        ])
        .build()
}

/// `imagecreatefromgif` — transcribed from the PHP form.
fn decl_fn_imagecreatefromgif() -> Stmt {
    function("imagecreatefromgif")
        .param("filename", TypeExpr::Str)
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_h", e_call("elephc_img_create_from_file", vec![e_var("filename"), e_int(3)])),
            s_if(
                e_binop(e_var("_h"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_binop(e_binop(e_str("imagecreatefromgif(): failed to open or decode '"), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("'"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_h")])),
        ])
        .build()
}

/// `imagecreatefrombmp` — transcribed from the PHP form.
fn decl_fn_imagecreatefrombmp() -> Stmt {
    function("imagecreatefrombmp")
        .param("filename", TypeExpr::Str)
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_h", e_call("elephc_img_create_from_file", vec![e_var("filename"), e_int(4)])),
            s_if(
                e_binop(e_var("_h"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_binop(e_binop(e_str("imagecreatefrombmp(): failed to open or decode '"), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("'"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_h")])),
        ])
        .build()
}

/// `imagecreatefromwebp` — transcribed from the PHP form.
fn decl_fn_imagecreatefromwebp() -> Stmt {
    function("imagecreatefromwebp")
        .param("filename", TypeExpr::Str)
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_h", e_call("elephc_img_create_from_file", vec![e_var("filename"), e_int(5)])),
            s_if(
                e_binop(e_var("_h"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_binop(e_binop(e_str("imagecreatefromwebp(): failed to open or decode '"), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("'"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_h")])),
        ])
        .build()
}

/// `imagecreatefromtga` — transcribed from the PHP form.
fn decl_fn_imagecreatefromtga() -> Stmt {
    function("imagecreatefromtga")
        .param("filename", TypeExpr::Str)
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_h", e_call("elephc_img_create_from_file", vec![e_var("filename"), e_int(6)])),
            s_if(
                e_binop(e_var("_h"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_binop(e_binop(e_str("imagecreatefromtga(): failed to open or decode '"), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("'"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_h")])),
        ])
        .build()
}

/// `imagecreatefromstring` — transcribed from the PHP form.
fn decl_fn_imagecreatefromstring() -> Stmt {
    function("imagecreatefromstring")
        .param("data", TypeExpr::Str)
        .returns(t_class("GdImage"))
        .body(vec![
            s_assign("_len", e_call("strlen", vec![e_var("data")])),
            s_if(
                e_binop(e_var("_len"), BinOp::LtEq, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_str("imagecreatefromstring(): empty image data")])),
                ],
                vec![],
                None,
            ),
            s_assign("_buf", e_call("elephc_img_stage_ptr", vec![e_var("_len")])),
            s_if(
                e_call("__elephc_ptr_is_null", vec![e_var("_buf")]),
                vec![
                    s_throw(e_new("ImageException", vec![e_str("imagecreatefromstring(): could not allocate decode buffer")])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_ptr_write_string", vec![e_var("_buf"), e_var("data")])),
            s_assign("_h", e_call("elephc_img_create_from_stage", vec![e_var("_len")])),
            s_if(
                e_binop(e_var("_h"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ImageException", vec![e_str("imagecreatefromstring(): data is not a recognized image")])),
                ],
                vec![],
                None,
            ),
            s_return(e_new("GdImage", vec![e_var("_h")])),
        ])
        .build()
}

/// `_elephc_img_output` — transcribed from the PHP form.
fn decl_fn_elephc_img_output() -> Stmt {
    function("_elephc_img_output")
        .param("handle", TypeExpr::Int)
        .param("fmt", TypeExpr::Int)
        .param("file", t_nullable(TypeExpr::Str))
        .param("quality", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("file"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_return(e_binop(e_call("elephc_img_write_file", vec![e_var("handle"), e_var("fmt"), e_cast(CastType::String, e_var("file")), e_var("quality")]), BinOp::StrictEq, e_int(0))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_img_encode", vec![e_var("handle"), e_var("fmt"), e_var("quality")]), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_len", e_call("elephc_img_encoded_len", vec![])),
            s_assign("_ptr", e_call("elephc_img_encoded_ptr", vec![])),
            s_assign("_bytes", e_call("__elephc_ptr_read_string", vec![e_var("_ptr"), e_var("_len")])),
            s_expr(e_call("elephc_img_encoded_clear", vec![])),
            s_echo(e_var("_bytes")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `imagepng` — transcribed from the PHP form.
fn decl_fn_imagepng() -> Stmt {
    function("imagepng")
        .param("image", t_class("GdImage"))
        .param_default("file", t_nullable(TypeExpr::Str), e_null())
        .param_default("quality", TypeExpr::Int, e_neg(e_int(1)))
        .param_default("filters", TypeExpr::Int, e_neg(e_int(1)))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_unused", e_var("filters")),
            s_return(e_call("_elephc_img_output", vec![e_prop(e_var("image"), "handle"), e_int(1), e_var("file"), e_var("quality")])),
        ])
        .build()
}

/// `imagejpeg` — transcribed from the PHP form.
fn decl_fn_imagejpeg() -> Stmt {
    function("imagejpeg")
        .param("image", t_class("GdImage"))
        .param_default("file", t_nullable(TypeExpr::Str), e_null())
        .param_default("quality", TypeExpr::Int, e_neg(e_int(1)))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_call("_elephc_img_output", vec![e_prop(e_var("image"), "handle"), e_int(2), e_var("file"), e_var("quality")])),
        ])
        .build()
}

/// `imagegif` — transcribed from the PHP form.
fn decl_fn_imagegif() -> Stmt {
    function("imagegif")
        .param("image", t_class("GdImage"))
        .param_default("file", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_call("_elephc_img_output", vec![e_prop(e_var("image"), "handle"), e_int(3), e_var("file"), e_neg(e_int(1))])),
        ])
        .build()
}

/// `imagebmp` — transcribed from the PHP form.
fn decl_fn_imagebmp() -> Stmt {
    function("imagebmp")
        .param("image", t_class("GdImage"))
        .param_default("file", t_nullable(TypeExpr::Str), e_null())
        .param_default("compressed", TypeExpr::Bool, e_bool(true))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_unused", e_var("compressed")),
            s_return(e_call("_elephc_img_output", vec![e_prop(e_var("image"), "handle"), e_int(4), e_var("file"), e_neg(e_int(1))])),
        ])
        .build()
}

/// `imagewebp` — transcribed from the PHP form.
fn decl_fn_imagewebp() -> Stmt {
    function("imagewebp")
        .param("image", t_class("GdImage"))
        .param_default("file", t_nullable(TypeExpr::Str), e_null())
        .param_default("quality", TypeExpr::Int, e_neg(e_int(1)))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_call("_elephc_img_output", vec![e_prop(e_var("image"), "handle"), e_int(5), e_var("file"), e_var("quality")])),
        ])
        .build()
}

/// `imagetypes` — transcribed from the PHP form.
fn decl_fn_imagetypes() -> Stmt {
    function("imagetypes")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_const("IMG_GIF"), BinOp::BitOr, e_const("IMG_JPG")), BinOp::BitOr, e_const("IMG_PNG")), BinOp::BitOr, e_const("IMG_WEBP")), BinOp::BitOr, e_const("IMG_BMP"))),
        ])
        .build()
}

/// `gd_info` — transcribed from the PHP form.
fn decl_fn_gd_info() -> Stmt {
    function("gd_info")
        .returns(t_array())
        .body(vec![
            s_return(e_array_assoc(vec![(e_str("GD Version"), e_str("bundled (pure-Rust, 2.1.0 compatible)")), (e_str("FreeType Support"), e_bool(false)), (e_str("FreeType Linkage"), e_str("")), (e_str("GIF Read Support"), e_bool(true)), (e_str("GIF Create Support"), e_bool(true)), (e_str("JPEG Support"), e_bool(true)), (e_str("PNG Support"), e_bool(true)), (e_str("WBMP Support"), e_bool(false)), (e_str("XPM Support"), e_bool(false)), (e_str("XBM Support"), e_bool(false)), (e_str("WebP Support"), e_bool(true)), (e_str("BMP Support"), e_bool(true)), (e_str("AVIF Support"), e_bool(false)), (e_str("TGA Read Support"), e_bool(true)), (e_str("JIS-mapped Japanese Font Support"), e_bool(false))])),
        ])
        .build()
}

/// `image_type_to_mime_type` — transcribed from the PHP form.
fn decl_fn_image_type_to_mime_type() -> Stmt {
    function("image_type_to_mime_type")
        .param("image_type", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_switch(e_var("image_type"), vec![
                (vec![e_const("IMAGETYPE_GIF")], vec![
                    s_return(e_str("image/gif")),
                ]),
                (vec![e_const("IMAGETYPE_JPEG")], vec![
                    s_return(e_str("image/jpeg")),
                ]),
                (vec![e_const("IMAGETYPE_PNG")], vec![
                    s_return(e_str("image/png")),
                ]),
                (vec![e_const("IMAGETYPE_SWF")], vec![
                    s_return(e_str("application/x-shockwave-flash")),
                ]),
                (vec![e_const("IMAGETYPE_PSD")], vec![
                    s_return(e_str("image/psd")),
                ]),
                (vec![e_const("IMAGETYPE_BMP")], vec![
                    s_return(e_str("image/bmp")),
                ]),
                (vec![e_const("IMAGETYPE_TIFF_II")], vec![
                    s_return(e_str("image/tiff")),
                ]),
                (vec![e_const("IMAGETYPE_TIFF_MM")], vec![
                    s_return(e_str("image/tiff")),
                ]),
                (vec![e_const("IMAGETYPE_JPC")], vec![
                    s_return(e_str("application/octet-stream")),
                ]),
                (vec![e_const("IMAGETYPE_JP2")], vec![
                    s_return(e_str("image/jp2")),
                ]),
                (vec![e_const("IMAGETYPE_JPX")], vec![
                    s_return(e_str("application/octet-stream")),
                ]),
                (vec![e_const("IMAGETYPE_JB2")], vec![
                    s_return(e_str("application/octet-stream")),
                ]),
                (vec![e_const("IMAGETYPE_SWC")], vec![
                    s_return(e_str("application/x-shockwave-flash")),
                ]),
                (vec![e_const("IMAGETYPE_IFF")], vec![
                    s_return(e_str("image/iff")),
                ]),
                (vec![e_const("IMAGETYPE_WBMP")], vec![
                    s_return(e_str("image/vnd.wap.wbmp")),
                ]),
                (vec![e_const("IMAGETYPE_XBM")], vec![
                    s_return(e_str("image/xbm")),
                ]),
                (vec![e_const("IMAGETYPE_ICO")], vec![
                    s_return(e_str("image/vnd.microsoft.icon")),
                ]),
                (vec![e_const("IMAGETYPE_WEBP")], vec![
                    s_return(e_str("image/webp")),
                ]),
                (vec![e_const("IMAGETYPE_AVIF")], vec![
                    s_return(e_str("image/avif")),
                ]),
            ], Some(vec![
                s_return(e_str("application/octet-stream")),
            ])),
        ])
        .build()
}

/// `image_type_to_extension` — transcribed from the PHP form.
fn decl_fn_image_type_to_extension() -> Stmt {
    function("image_type_to_extension")
        .param("image_type", TypeExpr::Int)
        .param_default("include_dot", TypeExpr::Bool, e_bool(true))
        .body(vec![
            s_assign("ext", e_str("")),
            s_switch(e_var("image_type"), vec![
                (vec![e_const("IMAGETYPE_GIF")], vec![
                    s_assign("ext", e_str("gif")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_JPEG")], vec![
                    s_assign("ext", e_str("jpeg")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_PNG")], vec![
                    s_assign("ext", e_str("png")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_SWF")], vec![
                    s_assign("ext", e_str("swf")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_PSD")], vec![
                    s_assign("ext", e_str("psd")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_BMP")], vec![
                    s_assign("ext", e_str("bmp")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_TIFF_II")], vec![
                    s_assign("ext", e_str("tiff")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_TIFF_MM")], vec![
                    s_assign("ext", e_str("tiff")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_JPC")], vec![
                    s_assign("ext", e_str("jpc")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_JP2")], vec![
                    s_assign("ext", e_str("jp2")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_JPX")], vec![
                    s_assign("ext", e_str("jpx")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_JB2")], vec![
                    s_assign("ext", e_str("jb2")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_IFF")], vec![
                    s_assign("ext", e_str("iff")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_WBMP")], vec![
                    s_assign("ext", e_str("wbmp")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_XBM")], vec![
                    s_assign("ext", e_str("xbm")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_ICO")], vec![
                    s_assign("ext", e_str("ico")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_WEBP")], vec![
                    s_assign("ext", e_str("webp")),
                    s_break(1),
                ]),
                (vec![e_const("IMAGETYPE_AVIF")], vec![
                    s_assign("ext", e_str("avif")),
                    s_break(1),
                ]),
            ], Some(vec![
                s_return(e_bool(false)),
            ])),
            s_if(
                e_var("include_dot"),
                vec![
                    s_return(e_binop(e_str("."), BinOp::Concat, e_var("ext"))),
                ],
                vec![],
                None,
            ),
            s_return(e_var("ext")),
        ])
        .build()
}

/// `getimagesize` — transcribed from the PHP form.
fn decl_fn_getimagesize() -> Stmt {
    function("getimagesize")
        .param("filename", TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_img_probe_file", vec![e_var("filename")]), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("w", e_call("elephc_img_probe_width", vec![])),
            s_assign("h", e_call("elephc_img_probe_height", vec![])),
            s_assign("type", e_call("elephc_img_probe_type", vec![])),
            s_assign("bits", e_call("elephc_img_probe_bits", vec![])),
            s_assign("channels", e_call("elephc_img_probe_channels", vec![])),
            s_return(e_array_assoc(vec![(e_int(0), e_var("w")), (e_int(1), e_var("h")), (e_int(2), e_var("type")), (e_int(3), e_binop(e_binop(e_binop(e_binop(e_str("width=\""), BinOp::Concat, e_var("w")), BinOp::Concat, e_str("\" height=\"")), BinOp::Concat, e_var("h")), BinOp::Concat, e_str("\""))), (e_str("bits"), e_var("bits")), (e_str("channels"), e_var("channels")), (e_str("mime"), e_call("image_type_to_mime_type", vec![e_var("type")]))])),
        ])
        .build()
}

/// `getimagesizefromstring` — transcribed from the PHP form.
fn decl_fn_getimagesizefromstring() -> Stmt {
    function("getimagesizefromstring")
        .param("data", TypeExpr::Str)
        .body(vec![
            s_assign("_len", e_call("strlen", vec![e_var("data")])),
            s_if(
                e_binop(e_var("_len"), BinOp::LtEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_buf", e_call("elephc_img_stage_ptr", vec![e_var("_len")])),
            s_if(
                e_call("__elephc_ptr_is_null", vec![e_var("_buf")]),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_ptr_write_string", vec![e_var("_buf"), e_var("data")])),
            s_if(
                e_binop(e_call("elephc_img_probe_stage", vec![e_var("_len")]), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("w", e_call("elephc_img_probe_width", vec![])),
            s_assign("h", e_call("elephc_img_probe_height", vec![])),
            s_assign("type", e_call("elephc_img_probe_type", vec![])),
            s_assign("bits", e_call("elephc_img_probe_bits", vec![])),
            s_assign("channels", e_call("elephc_img_probe_channels", vec![])),
            s_return(e_array_assoc(vec![(e_int(0), e_var("w")), (e_int(1), e_var("h")), (e_int(2), e_var("type")), (e_int(3), e_binop(e_binop(e_binop(e_binop(e_str("width=\""), BinOp::Concat, e_var("w")), BinOp::Concat, e_str("\" height=\"")), BinOp::Concat, e_var("h")), BinOp::Concat, e_str("\""))), (e_str("bits"), e_var("bits")), (e_str("channels"), e_var("channels")), (e_str("mime"), e_call("image_type_to_mime_type", vec![e_var("type")]))])),
        ])
        .build()
}

/// `EXIF_USE_MBSTRING` — transcribed from the PHP form.
fn decl_const_exif_use_mbstring() -> Stmt {
    s_const("EXIF_USE_MBSTRING", e_int(0))
}

/// `exif_imagetype` — transcribed from the PHP form.
fn decl_fn_exif_imagetype() -> Stmt {
    function("exif_imagetype")
        .param("filename", TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_img_probe_file", vec![e_var("filename")]), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_img_probe_type", vec![])),
        ])
        .build()
}

/// `exif_tagname` — transcribed from the PHP form.
fn decl_fn_exif_tagname() -> Stmt {
    function("exif_tagname")
        .param("index", TypeExpr::Int)
        .body(vec![
            s_assign("_len", e_call("elephc_exif_tagname", vec![e_var("index")])),
            s_if(
                e_binop(e_var("_len"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_ptr_read_string", vec![e_call("elephc_img_out_ptr", vec![]), e_var("_len")])),
        ])
        .build()
}

/// `exif_read_data` — transcribed from the PHP form.
fn decl_fn_exif_read_data() -> Stmt {
    function("exif_read_data")
        .param("filename", TypeExpr::Str)
        .param_default("required_sections", t_nullable(TypeExpr::Str), e_null())
        .param_default("as_arrays", TypeExpr::Bool, e_bool(false))
        .param_default("read_thumbnail", TypeExpr::Bool, e_bool(false))
        .body(vec![
            s_assign("_unused_a", e_var("required_sections")),
            s_assign("_unused_b", e_var("as_arrays")),
            s_assign("_unused_c", e_var("read_thumbnail")),
            s_assign("_count", e_call("elephc_exif_read", vec![e_var("filename")])),
            s_if(
                e_binop(e_var("_count"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("result", e_array(vec![])),
            s_assign("_n", e_call("elephc_img_kv_count", vec![])),
            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                s_assign("_klen", e_call("elephc_img_kv_key", vec![e_var("_i")])),
                s_assign("_key", e_call("__elephc_ptr_read_string", vec![e_call("elephc_img_out_ptr", vec![]), e_var("_klen")])),
                s_assign("_vlen", e_call("elephc_img_kv_val", vec![e_var("_i")])),
                s_assign("_val", e_call("__elephc_ptr_read_string", vec![e_call("elephc_img_out_ptr", vec![]), e_var("_vlen")])),
                s_array_assign("result", e_var("_key"), e_var("_val")),
            ]),
            s_return(e_var("result")),
        ])
        .build()
}

/// `read_exif_data` — transcribed from the PHP form.
fn decl_fn_read_exif_data() -> Stmt {
    function("read_exif_data")
        .param("filename", TypeExpr::Str)
        .param_default("required_sections", t_nullable(TypeExpr::Str), e_null())
        .param_default("as_arrays", TypeExpr::Bool, e_bool(false))
        .param_default("read_thumbnail", TypeExpr::Bool, e_bool(false))
        .body(vec![
            s_return(e_call("exif_read_data", vec![e_var("filename"), e_var("required_sections"), e_var("as_arrays"), e_var("read_thumbnail")])),
        ])
        .build()
}

/// `exif_thumbnail` — transcribed from the PHP form.
fn decl_fn_exif_thumbnail() -> Stmt {
    function("exif_thumbnail")
        .param("filename", TypeExpr::Str)
        .param_by_ref_default("width", None, e_int(0))
        .param_by_ref_default("height", None, e_int(0))
        .param_by_ref_default("image_type", None, e_int(0))
        .body(vec![
            s_assign("_len", e_call("elephc_exif_thumbnail", vec![e_var("filename")])),
            s_if(
                e_binop(e_var("_len"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_bytes", e_call("__elephc_ptr_read_string", vec![e_call("elephc_img_out_ptr", vec![]), e_var("_len")])),
            s_assign("width", e_call("elephc_exif_thumb_width", vec![])),
            s_assign("height", e_call("elephc_exif_thumb_height", vec![])),
            s_assign("image_type", e_call("elephc_exif_thumb_type", vec![])),
            s_return(e_var("_bytes")),
        ])
        .build()
}

/// `iptcparse` — transcribed from the PHP form.
fn decl_fn_iptcparse() -> Stmt {
    function("iptcparse")
        .param("iptcblock", TypeExpr::Str)
        .body(vec![
            s_assign("_len", e_call("strlen", vec![e_var("iptcblock")])),
            s_if(
                e_binop(e_var("_len"), BinOp::LtEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_buf", e_call("elephc_img_in_ptr", vec![e_var("_len")])),
            s_if(
                e_call("__elephc_ptr_is_null", vec![e_var("_buf")]),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_ptr_write_string", vec![e_var("_buf"), e_var("iptcblock")])),
            s_assign("_keys", e_call("elephc_iptc_parse", vec![e_var("_len")])),
            s_if(
                e_binop(e_var("_keys"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("result", e_array(vec![])),
            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_keys"))), Some(s_expr(e_post_inc("_i"))), vec![
                s_assign("_klen", e_call("elephc_iptc_key", vec![e_var("_i")])),
                s_assign("_key", e_call("__elephc_ptr_read_string", vec![e_call("elephc_img_out_ptr", vec![]), e_var("_klen")])),
                s_assign("_nv", e_call("elephc_iptc_val_count", vec![e_var("_i")])),
                s_assign("_sub", e_array(vec![])),
                s_for(Some(s_assign("_j", e_int(0))), Some(e_binop(e_var("_j"), BinOp::Lt, e_var("_nv"))), Some(s_expr(e_post_inc("_j"))), vec![
                    s_assign("_vlen", e_call("elephc_iptc_val", vec![e_var("_i"), e_var("_j")])),
                    s_array_push("_sub", e_call("__elephc_ptr_read_string", vec![e_call("elephc_img_out_ptr", vec![]), e_var("_vlen")])),
                ]),
                s_array_assign("result", e_var("_key"), e_var("_sub")),
            ]),
            s_return(e_var("result")),
        ])
        .build()
}

/// `iptcembed` — transcribed from the PHP form.
fn decl_fn_iptcembed() -> Stmt {
    function("iptcembed")
        .param("iptcdata", TypeExpr::Str)
        .param("jpeg_file_name", TypeExpr::Str)
        .param_default("spool", TypeExpr::Int, e_int(0))
        .body(vec![
            s_assign("_len", e_call("strlen", vec![e_var("iptcdata")])),
            s_assign("_buf", e_call("elephc_img_in_ptr", vec![e_var("_len")])),
            s_if(
                e_call("__elephc_ptr_is_null", vec![e_var("_buf")]),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_ptr_write_string", vec![e_var("_buf"), e_var("iptcdata")])),
            s_assign("_outlen", e_call("elephc_iptc_embed", vec![e_var("jpeg_file_name"), e_var("_len")])),
            s_if(
                e_binop(e_var("_outlen"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_bytes", e_call("__elephc_ptr_read_string", vec![e_call("elephc_img_out_ptr", vec![]), e_var("_outlen")])),
            s_if(
                e_binop(e_var("spool"), BinOp::GtEq, e_int(2)),
                vec![
                    s_echo(e_var("_bytes")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_bytes")),
        ])
        .build()
}

/// `ImagickException` — transcribed from the PHP form.
fn decl_class_imagickexception() -> Stmt {
    class("ImagickException")
        .extends("Exception")
        .build()
}

/// `ImagickDrawException` — transcribed from the PHP form.
fn decl_class_imagickdrawexception() -> Stmt {
    class("ImagickDrawException")
        .extends("Exception")
        .build()
}

/// `ImagickPixelException` — transcribed from the PHP form.
fn decl_class_imagickpixelexception() -> Stmt {
    class("ImagickPixelException")
        .extends("Exception")
        .build()
}

/// `ImagickPixelIteratorException` — transcribed from the PHP form.
fn decl_class_imagickpixeliteratorexception() -> Stmt {
    class("ImagickPixelIteratorException")
        .extends("Exception")
        .build()
}

/// `ImagickKernelException` — transcribed from the PHP form.
fn decl_class_imagickkernelexception() -> Stmt {
    class("ImagickKernelException")
        .extends("Exception")
        .build()
}

/// `_imagick_hexval` — transcribed from the PHP form.
fn decl_fn_imagick_hexval() -> Stmt {
    function("_imagick_hexval")
        .param("hex", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_n", e_call("strlen", vec![e_var("hex")])),
            s_assign("_acc", e_int(0)),
            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                s_assign("_o", e_call("ord", vec![e_index(e_var("hex"), e_var("_i"))])),
                s_assign("_d", e_int(0)),
                s_if(
                    e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(57))),
                    vec![
                        s_assign("_d", e_binop(e_var("_o"), BinOp::Sub, e_int(48))),
                    ],
                    vec![
                    (e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(102))), vec![
                        s_assign("_d", e_binop(e_var("_o"), BinOp::Sub, e_int(87))),
                    ]),
                    (e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(70))), vec![
                        s_assign("_d", e_binop(e_var("_o"), BinOp::Sub, e_int(55))),
                    ]),
                ],
                    None,
                ),
                s_assign("_acc", e_binop(e_binop(e_var("_acc"), BinOp::Mul, e_int(16)), BinOp::Add, e_var("_d"))),
            ]),
            s_return(e_var("_acc")),
        ])
        .build()
}

/// `_imagick_color_name` — transcribed from the PHP form.
fn decl_fn_imagick_color_name() -> Stmt {
    function("_imagick_color_name")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_switch(e_var("name"), vec![
                (vec![e_str("black")], vec![
                    s_return(e_int(0)),
                ]),
                (vec![e_str("white")], vec![
                    s_return(e_int(16777215)),
                ]),
                (vec![e_str("red")], vec![
                    s_return(e_int(16711680)),
                ]),
                (vec![e_str("lime")], vec![
                    s_return(e_int(65280)),
                ]),
                (vec![e_str("green")], vec![
                    s_return(e_int(32768)),
                ]),
                (vec![e_str("blue")], vec![
                    s_return(e_int(255)),
                ]),
                (vec![e_str("yellow")], vec![
                    s_return(e_int(16776960)),
                ]),
                (vec![e_str("cyan"), e_str("aqua")], vec![
                    s_return(e_int(65535)),
                ]),
                (vec![e_str("magenta"), e_str("fuchsia")], vec![
                    s_return(e_int(16711935)),
                ]),
                (vec![e_str("silver")], vec![
                    s_return(e_int(12632256)),
                ]),
                (vec![e_str("gray"), e_str("grey")], vec![
                    s_return(e_int(8421504)),
                ]),
                (vec![e_str("maroon")], vec![
                    s_return(e_int(8388608)),
                ]),
                (vec![e_str("olive")], vec![
                    s_return(e_int(8421376)),
                ]),
                (vec![e_str("purple")], vec![
                    s_return(e_int(8388736)),
                ]),
                (vec![e_str("teal")], vec![
                    s_return(e_int(32896)),
                ]),
                (vec![e_str("navy")], vec![
                    s_return(e_int(128)),
                ]),
                (vec![e_str("orange")], vec![
                    s_return(e_int(16753920)),
                ]),
                (vec![e_str("pink")], vec![
                    s_return(e_int(16761035)),
                ]),
                (vec![e_str("brown")], vec![
                    s_return(e_int(10824234)),
                ]),
                (vec![e_str("gold")], vec![
                    s_return(e_int(16766720)),
                ]),
                (vec![e_str("violet")], vec![
                    s_return(e_int(15631086)),
                ]),
                (vec![e_str("indigo")], vec![
                    s_return(e_int(4915330)),
                ]),
                (vec![e_str("transparent"), e_str("none")], vec![
                    s_return(e_int(2130706432)),
                ]),
            ], Some(vec![
                s_return(e_neg(e_int(1))),
            ])),
        ])
        .build()
}

/// `_imagick_parse_color` — transcribed from the PHP form.
fn decl_fn_imagick_parse_color() -> Stmt {
    function("_imagick_parse_color")
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_c", e_call("trim", vec![e_var("c")])),
            s_if(
                e_binop(e_var("_c"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("ord", vec![e_index(e_var("_c"), e_int(0))]), BinOp::StrictEq, e_int(35)),
                vec![
                    s_assign("_hex", e_call("substr", vec![e_var("_c"), e_int(1)])),
                    s_assign("_len", e_call("strlen", vec![e_var("_hex")])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_assign("_r", e_call("_imagick_hexval", vec![e_binop(e_index(e_var("_hex"), e_int(0)), BinOp::Concat, e_index(e_var("_hex"), e_int(0)))])),
                            s_assign("_g", e_call("_imagick_hexval", vec![e_binop(e_index(e_var("_hex"), e_int(1)), BinOp::Concat, e_index(e_var("_hex"), e_int(1)))])),
                            s_assign("_b", e_call("_imagick_hexval", vec![e_binop(e_index(e_var("_hex"), e_int(2)), BinOp::Concat, e_index(e_var("_hex"), e_int(2)))])),
                            s_return(e_binop(e_binop(e_binop(e_var("_r"), BinOp::ShiftLeft, e_int(16)), BinOp::BitOr, e_binop(e_var("_g"), BinOp::ShiftLeft, e_int(8))), BinOp::BitOr, e_var("_b"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_len"), BinOp::StrictEq, e_int(6)),
                        vec![
                            s_assign("_r", e_call("_imagick_hexval", vec![e_call("substr", vec![e_var("_hex"), e_int(0), e_int(2)])])),
                            s_assign("_g", e_call("_imagick_hexval", vec![e_call("substr", vec![e_var("_hex"), e_int(2), e_int(2)])])),
                            s_assign("_b", e_call("_imagick_hexval", vec![e_call("substr", vec![e_var("_hex"), e_int(4), e_int(2)])])),
                            s_return(e_binop(e_binop(e_binop(e_var("_r"), BinOp::ShiftLeft, e_int(16)), BinOp::BitOr, e_binop(e_var("_g"), BinOp::ShiftLeft, e_int(8))), BinOp::BitOr, e_var("_b"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_len"), BinOp::StrictEq, e_int(8)),
                        vec![
                            s_assign("_r", e_call("_imagick_hexval", vec![e_call("substr", vec![e_var("_hex"), e_int(0), e_int(2)])])),
                            s_assign("_g", e_call("_imagick_hexval", vec![e_call("substr", vec![e_var("_hex"), e_int(2), e_int(2)])])),
                            s_assign("_b", e_call("_imagick_hexval", vec![e_call("substr", vec![e_var("_hex"), e_int(4), e_int(2)])])),
                            s_assign("_a8", e_call("_imagick_hexval", vec![e_call("substr", vec![e_var("_hex"), e_int(6), e_int(2)])])),
                            s_assign("_gd", e_cast(CastType::Int, e_binop(e_binop(e_binop(e_int(255), BinOp::Sub, e_var("_a8")), BinOp::Mul, e_int(127)), BinOp::Div, e_int(255)))),
                            s_return(e_binop(e_binop(e_binop(e_binop(e_var("_gd"), BinOp::ShiftLeft, e_int(24)), BinOp::BitOr, e_binop(e_var("_r"), BinOp::ShiftLeft, e_int(16))), BinOp::BitOr, e_binop(e_var("_g"), BinOp::ShiftLeft, e_int(8))), BinOp::BitOr, e_var("_b"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("ImagickPixelException", vec![e_binop(e_binop(e_str("ImagickPixel: malformed hex color '"), BinOp::Concat, e_var("c")), BinOp::Concat, e_str("'"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_lower", e_call("strtolower", vec![e_var("_c")])),
            s_if(
                e_binop(e_binop(e_call("substr", vec![e_var("_lower"), e_int(0), e_int(4)]), BinOp::StrictEq, e_str("rgb(")), BinOp::Or, e_binop(e_call("substr", vec![e_var("_lower"), e_int(0), e_int(5)]), BinOp::StrictEq, e_str("rgba("))),
                vec![
                    s_assign("_open", e_call("strpos", vec![e_var("_c"), e_str("(")])),
                    s_assign("_close", e_call("strpos", vec![e_var("_c"), e_str(")")])),
                    s_assign("_inner", e_call("substr", vec![e_var("_c"), e_binop(e_var("_open"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("_close"), BinOp::Sub, e_var("_open")), BinOp::Sub, e_int(1))])),
                    s_assign("_parts", e_call("explode", vec![e_str(","), e_var("_inner")])),
                    s_assign("_r", e_cast(CastType::Int, e_call("trim", vec![e_index(e_var("_parts"), e_int(0))]))),
                    s_assign("_g", e_cast(CastType::Int, e_call("trim", vec![e_index(e_var("_parts"), e_int(1))]))),
                    s_assign("_b", e_cast(CastType::Int, e_call("trim", vec![e_index(e_var("_parts"), e_int(2))]))),
                    s_assign("_gd", e_int(0)),
                    s_if(
                        e_binop(e_call("count", vec![e_var("_parts")]), BinOp::GtEq, e_int(4)),
                        vec![
                            s_assign("_af", e_cast(CastType::Float, e_call("trim", vec![e_index(e_var("_parts"), e_int(3))]))),
                            s_assign("_gd", e_cast(CastType::Int, e_binop(e_binop(e_float(1.0), BinOp::Sub, e_var("_af")), BinOp::Mul, e_int(127)))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_binop(e_binop(e_binop(e_var("_gd"), BinOp::ShiftLeft, e_int(24)), BinOp::BitOr, e_binop(e_var("_r"), BinOp::ShiftLeft, e_int(16))), BinOp::BitOr, e_binop(e_var("_g"), BinOp::ShiftLeft, e_int(8))), BinOp::BitOr, e_var("_b"))),
                ],
                vec![],
                None,
            ),
            s_assign("_named", e_call("_imagick_color_name", vec![e_var("_lower")])),
            s_if(
                e_binop(e_var("_named"), BinOp::GtEq, e_int(0)),
                vec![
                    s_return(e_var("_named")),
                ],
                vec![],
                None,
            ),
            s_throw(e_new("ImagickPixelException", vec![e_binop(e_binop(e_str("ImagickPixel: unrecognized color '"), BinOp::Concat, e_var("c")), BinOp::Concat, e_str("'"))])),
        ])
        .build()
}

/// `_imagick_norm_color` — transcribed from the PHP form.
fn decl_fn_imagick_norm_color() -> Stmt {
    function("_imagick_norm_color")
        .param_untyped("color")
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_call("is_string", vec![e_var("color")]),
                vec![
                    s_return(e_call("_imagick_parse_color", vec![e_var("color")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("color"), "ImagickPixel"),
                vec![
                    s_return(e_cast(CastType::Int, e_prop(e_var("color"), "packed"))),
                ],
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
        .build()
}

/// `_imagick_fmt_to_code` — transcribed from the PHP form.
fn decl_fn_imagick_fmt_to_code() -> Stmt {
    function("_imagick_fmt_to_code")
        .param("format", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_f", e_call("strtoupper", vec![e_var("format")])),
            s_if(
                e_binop(e_var("_f"), BinOp::StrictEq, e_str("PNG")),
                vec![
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("_f"), BinOp::StrictEq, e_str("JPEG")), BinOp::Or, e_binop(e_var("_f"), BinOp::StrictEq, e_str("JPG"))),
                vec![
                    s_return(e_int(2)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_f"), BinOp::StrictEq, e_str("GIF")),
                vec![
                    s_return(e_int(3)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_f"), BinOp::StrictEq, e_str("BMP")),
                vec![
                    s_return(e_int(4)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_f"), BinOp::StrictEq, e_str("WEBP")),
                vec![
                    s_return(e_int(5)),
                ],
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
        .build()
}

/// `_imagick_code_to_fmt` — transcribed from the PHP form.
fn decl_fn_imagick_code_to_fmt() -> Stmt {
    function("_imagick_code_to_fmt")
        .param("code", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("code"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_return(e_str("PNG")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("code"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_return(e_str("JPEG")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("code"), BinOp::StrictEq, e_int(3)),
                vec![
                    s_return(e_str("GIF")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("code"), BinOp::StrictEq, e_int(4)),
                vec![
                    s_return(e_str("BMP")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("code"), BinOp::StrictEq, e_int(5)),
                vec![
                    s_return(e_str("WEBP")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `_imagick_fmt_from_path` — transcribed from the PHP form.
fn decl_fn_imagick_fmt_from_path() -> Stmt {
    function("_imagick_fmt_from_path")
        .param("path", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_dot", e_call("strrpos", vec![e_var("path"), e_str(".")])),
            s_if(
                e_binop(e_var("_dot"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("_imagick_fmt_to_code", vec![e_call("substr", vec![e_var("path"), e_binop(e_var("_dot"), BinOp::Add, e_int(1))])])),
        ])
        .build()
}

/// `_imagick_pack2` — transcribed from the PHP form.
fn decl_fn_imagick_pack2() -> Stmt {
    function("_imagick_pack2")
        .param("hi", TypeExpr::Int)
        .param("lo", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_var("hi"), BinOp::BitAnd, e_int(4294967295)), BinOp::ShiftLeft, e_int(32)), BinOp::BitOr, e_binop(e_var("lo"), BinOp::BitAnd, e_int(4294967295)))),
        ])
        .build()
}

/// `_imagick_pixel_from_int` — transcribed from the PHP form.
fn decl_fn_imagick_pixel_from_int() -> Stmt {
    function("_imagick_pixel_from_int")
        .param("packed", TypeExpr::Int)
        .returns(t_class("ImagickPixel"))
        .body(vec![
            s_assign("_p", e_new("ImagickPixel", vec![e_str("black")])),
            s_prop_assign(e_var("_p"), "packed", e_var("packed")),
            s_return(e_var("_p")),
        ])
        .build()
}

/// `ImagickPixel` — transcribed from the PHP form.
fn decl_class_imagickpixel() -> Stmt {
    class("ImagickPixel")
        .prop("packed", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param_default("color", TypeExpr::Str, e_str("black"))
                .body(vec![
                    s_prop_assign(e_this(), "packed", e_call("_imagick_parse_color", vec![e_var("color")])),
                ]),
        )
        .method(
            method("setColor")
                .param("color", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "packed", e_call("_imagick_parse_color", vec![e_var("color")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("getColor")
                .param_default("normalized", TypeExpr::Int, e_int(0))
                .body(vec![
                    s_assign("_r", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))),
                    s_assign("_g", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))),
                    s_assign("_b", e_binop(e_this_prop("packed"), BinOp::BitAnd, e_int(255))),
                    s_assign("_gd", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(24)), BinOp::BitAnd, e_int(127))),
                    s_assign("_a", e_binop(e_int(255), BinOp::Sub, e_cast(CastType::Int, e_binop(e_binop(e_var("_gd"), BinOp::Mul, e_int(255)), BinOp::Div, e_int(127))))),
                    s_if(
                        e_binop(e_var("normalized"), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("r"), e_binop(e_var("_r"), BinOp::Div, e_int(255))), (e_str("g"), e_binop(e_var("_g"), BinOp::Div, e_int(255))), (e_str("b"), e_binop(e_var("_b"), BinOp::Div, e_int(255))), (e_str("a"), e_binop(e_var("_a"), BinOp::Div, e_int(255)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array_assoc(vec![(e_str("r"), e_var("_r")), (e_str("g"), e_var("_g")), (e_str("b"), e_var("_b")), (e_str("a"), e_var("_a"))])),
                ]),
        )
        .method(
            method("getColorValue")
                .param("color", TypeExpr::Int)
                .returns(TypeExpr::Float)
                .body(vec![
                    s_assign("_r", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))),
                    s_assign("_g", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))),
                    s_assign("_b", e_binop(e_this_prop("packed"), BinOp::BitAnd, e_int(255))),
                    s_assign("_gd", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(24)), BinOp::BitAnd, e_int(127))),
                    s_assign("_a", e_binop(e_int(255), BinOp::Sub, e_cast(CastType::Int, e_binop(e_binop(e_var("_gd"), BinOp::Mul, e_int(255)), BinOp::Div, e_int(127))))),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_return(e_binop(e_var("_r"), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_return(e_binop(e_var("_g"), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_return(e_binop(e_var("_b"), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(8)),
                        vec![
                            s_return(e_binop(e_var("_a"), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(7)),
                        vec![
                            s_return(e_binop(e_binop(e_int(255), BinOp::Sub, e_var("_a")), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_float(0.0)),
                ]),
        )
        .method(
            method("getColorAsString")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_r", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))),
                    s_assign("_g", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))),
                    s_assign("_b", e_binop(e_this_prop("packed"), BinOp::BitAnd, e_int(255))),
                    s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("srgb("), BinOp::Concat, e_var("_r")), BinOp::Concat, e_str(",")), BinOp::Concat, e_var("_g")), BinOp::Concat, e_str(",")), BinOp::Concat, e_var("_b")), BinOp::Concat, e_str(")"))),
                ]),
        )
        .method(
            method("isSimilar")
                .param("color", t_class("ImagickPixel"))
                .param("fuzz", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_dr", e_binop(e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255)), BinOp::Sub, e_binop(e_binop(e_prop(e_var("color"), "packed"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255)))),
                    s_assign("_dg", e_binop(e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255)), BinOp::Sub, e_binop(e_binop(e_prop(e_var("color"), "packed"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255)))),
                    s_assign("_db", e_binop(e_binop(e_this_prop("packed"), BinOp::BitAnd, e_int(255)), BinOp::Sub, e_binop(e_prop(e_var("color"), "packed"), BinOp::BitAnd, e_int(255)))),
                    s_assign("_dist", e_binop(e_call("sqrt", vec![e_cast(CastType::Float, e_binop(e_binop(e_binop(e_var("_dr"), BinOp::Mul, e_var("_dr")), BinOp::Add, e_binop(e_var("_dg"), BinOp::Mul, e_var("_dg"))), BinOp::Add, e_binop(e_var("_db"), BinOp::Mul, e_var("_db"))))]), BinOp::Div, e_binop(e_call("sqrt", vec![e_float(3.0)]), BinOp::Mul, e_int(255)))),
                    s_return(e_binop(e_var("_dist"), BinOp::LtEq, e_var("fuzz"))),
                ]),
        )
        .method(
            method("isPixelSimilar")
                .param("color", t_class("ImagickPixel"))
                .param("fuzz", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "isSimilar", vec![e_var("color"), e_var("fuzz")])),
                ]),
        )
        .method(
            method("clear")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("destroy")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(true)),
                ]),
        )
        // --- begin auto-generated API-surface throwing stubs (do not edit; regen via crates/elephc-image/tools/gen_image_api_stubs.py) ---
        .method(
            method("getColorCount")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::getColorCount() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getColorQuantum")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::getColorQuantum() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getColorValueQuantum")
                .param("color", TypeExpr::Int)
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
                .body(vec![
                    s_assign("_u_color", e_var("color")),
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::getColorValueQuantum() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getHSL")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::getHSL() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getIndex")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::getIndex() is not supported in elephc")])),
                ]),
        )
        .method(
            method("isPixelSimilarQuantum")
                .param("color", TypeExpr::Str)
                .param_default("fuzz", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_color", e_var("color")),
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::isPixelSimilarQuantum() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setcolorcount")
                .param("colorCount", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_colorCount", e_var("colorCount")),
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::setcolorcount() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setColorValue")
                .param("color", TypeExpr::Int)
                .param("value", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_color", e_var("color")),
                    s_assign("_u_value", e_var("value")),
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::setColorValue() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setColorValueQuantum")
                .param("color", TypeExpr::Int)
                .param("value", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_color", e_var("color")),
                    s_assign("_u_value", e_var("value")),
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::setColorValueQuantum() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setHSL")
                .param("hue", TypeExpr::Float)
                .param("saturation", TypeExpr::Float)
                .param("luminosity", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_hue", e_var("hue")),
                    s_assign("_u_saturation", e_var("saturation")),
                    s_assign("_u_luminosity", e_var("luminosity")),
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::setHSL() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setIndex")
                .param("index", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_index", e_var("index")),
                    s_throw(e_new("ImagickPixelException", vec![e_str("ImagickPixel::setIndex() is not supported in elephc")])),
                ]),
        )
        // --- end auto-generated API-surface stubs ---
        .build()
}

/// `ImagickKernel` — transcribed from the PHP form.
fn decl_class_imagickkernel() -> Stmt {
    class("ImagickKernel")
        .prop("size", TypeExpr::Int, Some(e_int(0)))
        .prop("values", t_array(), Some(e_array(vec![])))
        .prop("divisor", TypeExpr::Float, Some(e_float(1.0)))
        .method(
            method("fromMatrix")
                .static_()
                .param("matrix", t_array())
                .returns(t_class("ImagickKernel"))
                .body(vec![
                    s_assign("_k", e_new("ImagickKernel", vec![])),
                    s_assign("_n", e_call("count", vec![e_var("matrix")])),
                    s_prop_assign(e_var("_k"), "size", e_var("_n")),
                    s_assign("_sum", e_float(0.0)),
                    s_for(Some(s_assign("_r", e_int(0))), Some(e_binop(e_var("_r"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_r"))), vec![
                        s_for(Some(s_assign("_c", e_int(0))), Some(e_binop(e_var("_c"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_c"))), vec![
                            s_assign("_v", e_cast(CastType::Float, e_index(e_index(e_var("matrix"), e_var("_r")), e_var("_c")))),
                            s_prop_array_push(e_var("_k"), "values", e_var("_v")),
                            s_assign("_sum", e_binop(e_var("_sum"), BinOp::Add, e_var("_v"))),
                        ]),
                    ]),
                    s_prop_assign(e_var("_k"), "divisor", e_ternary(e_binop(e_var("_sum"), BinOp::Eq, e_float(0.0)), e_float(1.0), e_var("_sum"))),
                    s_return(e_var("_k")),
                ]),
        )
        .method(
            method("fromBuiltIn")
                .static_()
                .param("kernelType", TypeExpr::Int)
                .param("kernelString", TypeExpr::Str)
                .returns(t_class("ImagickKernel"))
                .body(vec![
                    s_assign("_u_t", e_var("kernelType")),
                    s_assign("_u_s", e_var("kernelString")),
                    s_throw(e_new("ImagickKernelException", vec![e_str("ImagickKernel::fromBuiltIn() is not supported in elephc; use fromMatrix()")])),
                ]),
        )
        .method(
            method("getMatrix")
                .returns(t_array())
                .body(vec![
                    s_return(e_this_prop("values")),
                ]),
        )
        .method(
            method("_size")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("size")),
                ]),
        )
        .method(
            method("_at")
                .param("i", TypeExpr::Int)
                .returns(TypeExpr::Float)
                .body(vec![
                    s_return(e_cast(CastType::Float, e_index(e_this_prop("values"), e_var("i")))),
                ]),
        )
        .method(
            method("_divisor")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_return(e_this_prop("divisor")),
                ]),
        )
        // --- begin auto-generated API-surface throwing stubs (do not edit; regen via crates/elephc-image/tools/gen_image_api_stubs.py) ---
        .method(
            method("addKernel")
                .param("imagickKernel", t_class("ImagickKernel"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_u_imagickKernel", e_var("imagickKernel")),
                    s_throw(e_new("ImagickKernelException", vec![e_str("ImagickKernel::addKernel() is not supported in elephc")])),
                ]),
        )
        .method(
            method("addUnityKernel")
                .param("scale", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_u_scale", e_var("scale")),
                    s_throw(e_new("ImagickKernelException", vec![e_str("ImagickKernel::addUnityKernel() is not supported in elephc")])),
                ]),
        )
        .method(
            method("scale")
                .param("scale", TypeExpr::Float)
                .param_default("normalizeFlag", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_u_scale", e_var("scale")),
                    s_assign("_u_normalizeFlag", e_var("normalizeFlag")),
                    s_throw(e_new("ImagickKernelException", vec![e_str("ImagickKernel::scale() is not supported in elephc")])),
                ]),
        )
        .method(
            method("separate")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickKernelException", vec![e_str("ImagickKernel::separate() is not supported in elephc")])),
                ]),
        )
        // --- end auto-generated API-surface stubs ---
        .build()
}

/// `ImagickDraw` — transcribed from the PHP form.
fn decl_class_imagickdraw() -> Stmt {
    class("ImagickDraw")
        .private_prop("draw", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .body(vec![
                    s_prop_assign(e_this(), "draw", e_call("elephc_idraw_new", vec![])),
                ]),
        )
        .method(
            method("_imagickHandle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("draw")),
                ]),
        )
        .method(
            method("setFillColor")
                .param_untyped("fill")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_set_fill", vec![e_this_prop("draw"), e_call("_imagick_norm_color", vec![e_var("fill")])])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("setStrokeColor")
                .param_untyped("stroke")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_set_stroke", vec![e_this_prop("draw"), e_call("_imagick_norm_color", vec![e_var("stroke")])])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("setStrokeWidth")
                .param("width", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_set_stroke_width", vec![e_this_prop("draw"), e_cast(CastType::Int, e_call("round", vec![e_var("width")]))])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("getFillColor")
                .returns(t_class("ImagickPixel"))
                .body(vec![
                    s_return(e_call("_imagick_pixel_from_int", vec![e_call("elephc_idraw_get_fill", vec![e_this_prop("draw")])])),
                ]),
        )
        .method(
            method("line")
                .param_untyped("sx")
                .param_untyped("sy")
                .param_untyped("ex")
                .param_untyped("ey")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_line", vec![e_this_prop("draw"), e_cast(CastType::Int, e_call("round", vec![e_var("sx")])), e_cast(CastType::Int, e_call("round", vec![e_var("sy")])), e_cast(CastType::Int, e_call("round", vec![e_var("ex")])), e_cast(CastType::Int, e_call("round", vec![e_var("ey")]))])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("rectangle")
                .param_untyped("x1")
                .param_untyped("y1")
                .param_untyped("x2")
                .param_untyped("y2")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_rectangle", vec![e_this_prop("draw"), e_cast(CastType::Int, e_call("round", vec![e_var("x1")])), e_cast(CastType::Int, e_call("round", vec![e_var("y1")])), e_cast(CastType::Int, e_call("round", vec![e_var("x2")])), e_cast(CastType::Int, e_call("round", vec![e_var("y2")]))])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("circle")
                .param_untyped("ox")
                .param_untyped("oy")
                .param_untyped("px")
                .param_untyped("py")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_circle", vec![e_this_prop("draw"), e_cast(CastType::Int, e_call("round", vec![e_var("ox")])), e_cast(CastType::Int, e_call("round", vec![e_var("oy")])), e_cast(CastType::Int, e_call("round", vec![e_var("px")])), e_cast(CastType::Int, e_call("round", vec![e_var("py")]))])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("ellipse")
                .param_untyped("ox")
                .param_untyped("oy")
                .param_untyped("rx")
                .param_untyped("ry")
                .param_untyped("start")
                .param_untyped("end")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_oxy", e_call("_imagick_pack2", vec![e_cast(CastType::Int, e_call("round", vec![e_var("ox")])), e_cast(CastType::Int, e_call("round", vec![e_var("oy")]))])),
                    s_assign("_rxy", e_call("_imagick_pack2", vec![e_cast(CastType::Int, e_call("round", vec![e_var("rx")])), e_cast(CastType::Int, e_call("round", vec![e_var("ry")]))])),
                    s_assign("_se", e_call("_imagick_pack2", vec![e_cast(CastType::Int, e_call("round", vec![e_var("start")])), e_cast(CastType::Int, e_call("round", vec![e_var("end")]))])),
                    s_expr(e_call("elephc_idraw_ellipse", vec![e_this_prop("draw"), e_var("_oxy"), e_var("_rxy"), e_var("_se")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("point")
                .param_untyped("x")
                .param_untyped("y")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_point", vec![e_this_prop("draw"), e_cast(CastType::Int, e_call("round", vec![e_var("x")])), e_cast(CastType::Int, e_call("round", vec![e_var("y")]))])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("polygon")
                .param("coordinates", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_poly_reset", vec![e_this_prop("draw")])),
                    s_assign("_n", e_call("count", vec![e_var("coordinates")])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_px", e_cast(CastType::Int, e_call("round", vec![e_index(e_index(e_var("coordinates"), e_var("_i")), e_str("x"))]))),
                        s_assign("_py", e_cast(CastType::Int, e_call("round", vec![e_index(e_index(e_var("coordinates"), e_var("_i")), e_str("y"))]))),
                        s_expr(e_call("elephc_idraw_poly_point", vec![e_this_prop("draw"), e_var("_px"), e_var("_py")])),
                    ]),
                    s_expr(e_call("elephc_idraw_polygon", vec![e_this_prop("draw")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("clear")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_clear", vec![e_this_prop("draw")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("destroy")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_destroy", vec![e_this_prop("draw")])),
                    s_prop_assign(e_this(), "draw", e_int(0)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_expr(e_call("elephc_idraw_destroy", vec![e_this_prop("draw")])),
                ]),
        )
        // --- begin auto-generated API-surface throwing stubs (do not edit; regen via crates/elephc-image/tools/gen_image_api_stubs.py) ---
        .method(
            method("affine")
                .param("affine", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_affine", e_var("affine")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::affine() is not supported in elephc")])),
                ]),
        )
        .method(
            method("annotation")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_text", e_var("text")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::annotation() is not supported in elephc")])),
                ]),
        )
        .method(
            method("arc")
                .param("start_x", TypeExpr::Float)
                .param("start_y", TypeExpr::Float)
                .param("end_x", TypeExpr::Float)
                .param("end_y", TypeExpr::Float)
                .param("start_angle", TypeExpr::Float)
                .param("end_angle", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_start_x", e_var("start_x")),
                    s_assign("_u_start_y", e_var("start_y")),
                    s_assign("_u_end_x", e_var("end_x")),
                    s_assign("_u_end_y", e_var("end_y")),
                    s_assign("_u_start_angle", e_var("start_angle")),
                    s_assign("_u_end_angle", e_var("end_angle")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::arc() is not supported in elephc")])),
                ]),
        )
        .method(
            method("bezier")
                .param("coordinates", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_coordinates", e_var("coordinates")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::bezier() is not supported in elephc")])),
                ]),
        )
        .method(
            method("clone")
                .returns(t_class("ImagickDraw"))
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::clone() is not supported in elephc")])),
                ]),
        )
        .method(
            method("color")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .param("paint", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_paint", e_var("paint")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::color() is not supported in elephc")])),
                ]),
        )
        .method(
            method("comment")
                .param("comment", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_comment", e_var("comment")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::comment() is not supported in elephc")])),
                ]),
        )
        .method(
            method("composite")
                .param("composite", TypeExpr::Int)
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .param("width", TypeExpr::Float)
                .param("height", TypeExpr::Float)
                .param("image", t_class("Imagick"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_composite", e_var("composite")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_image", e_var("image")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::composite() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getClipPath")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getClipPath() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getClipRule")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getClipRule() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getClipUnits")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getClipUnits() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFillOpacity")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getFillOpacity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFillRule")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getFillRule() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFont")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getFont() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFontFamily")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getFontFamily() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFontSize")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getFontSize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFontStretch")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getFontStretch() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFontStyle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getFontStyle() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFontWeight")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getFontWeight() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getGravity")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getGravity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getStrokeAntialias")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getStrokeAntialias() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getStrokeColor")
                .returns(t_class("ImagickPixel"))
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getStrokeColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getStrokeDashArray")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getStrokeDashArray() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getStrokeDashOffset")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getStrokeDashOffset() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getStrokeLineCap")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getStrokeLineCap() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getStrokeLineJoin")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getStrokeLineJoin() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getStrokeMiterLimit")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getStrokeMiterLimit() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getStrokeOpacity")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getStrokeOpacity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getStrokeWidth")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getStrokeWidth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getTextAlignment")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getTextAlignment() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getTextAntialias")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getTextAntialias() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getTextDecoration")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getTextDecoration() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getTextEncoding")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getTextEncoding() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getTextInterlineSpacing")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getTextInterlineSpacing() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getTextInterwordSpacing")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getTextInterwordSpacing() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getTextKerning")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getTextKerning() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getTextUnderColor")
                .returns(t_class("ImagickPixel"))
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getTextUnderColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getVectorGraphics")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::getVectorGraphics() is not supported in elephc")])),
                ]),
        )
        .method(
            method("matte")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .param("paint", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_paint", e_var("paint")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::matte() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathClose")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathClose() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathCurveToAbsolute")
                .param("x1", TypeExpr::Float)
                .param("y1", TypeExpr::Float)
                .param("x2", TypeExpr::Float)
                .param("y2", TypeExpr::Float)
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x1", e_var("x1")),
                    s_assign("_u_y1", e_var("y1")),
                    s_assign("_u_x2", e_var("x2")),
                    s_assign("_u_y2", e_var("y2")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathCurveToAbsolute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathCurveToQuadraticBezierAbsolute")
                .param("x1", TypeExpr::Float)
                .param("y1", TypeExpr::Float)
                .param("x_end", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x1", e_var("x1")),
                    s_assign("_u_y1", e_var("y1")),
                    s_assign("_u_x_end", e_var("x_end")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathCurveToQuadraticBezierAbsolute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathCurveToQuadraticBezierRelative")
                .param("x1", TypeExpr::Float)
                .param("y1", TypeExpr::Float)
                .param("x_end", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x1", e_var("x1")),
                    s_assign("_u_y1", e_var("y1")),
                    s_assign("_u_x_end", e_var("x_end")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathCurveToQuadraticBezierRelative() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathCurveToQuadraticBezierSmoothAbsolute")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathCurveToQuadraticBezierSmoothAbsolute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathCurveToQuadraticBezierSmoothRelative")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathCurveToQuadraticBezierSmoothRelative() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathCurveToRelative")
                .param("x1", TypeExpr::Float)
                .param("y1", TypeExpr::Float)
                .param("x2", TypeExpr::Float)
                .param("y2", TypeExpr::Float)
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x1", e_var("x1")),
                    s_assign("_u_y1", e_var("y1")),
                    s_assign("_u_x2", e_var("x2")),
                    s_assign("_u_y2", e_var("y2")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathCurveToRelative() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathCurveToSmoothAbsolute")
                .param("x2", TypeExpr::Float)
                .param("y2", TypeExpr::Float)
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x2", e_var("x2")),
                    s_assign("_u_y2", e_var("y2")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathCurveToSmoothAbsolute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathCurveToSmoothRelative")
                .param("x2", TypeExpr::Float)
                .param("y2", TypeExpr::Float)
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x2", e_var("x2")),
                    s_assign("_u_y2", e_var("y2")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathCurveToSmoothRelative() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathEllipticArcAbsolute")
                .param("rx", TypeExpr::Float)
                .param("ry", TypeExpr::Float)
                .param("x_axis_rotation", TypeExpr::Float)
                .param("large_arc", TypeExpr::Bool)
                .param("sweep", TypeExpr::Bool)
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_rx", e_var("rx")),
                    s_assign("_u_ry", e_var("ry")),
                    s_assign("_u_x_axis_rotation", e_var("x_axis_rotation")),
                    s_assign("_u_large_arc", e_var("large_arc")),
                    s_assign("_u_sweep", e_var("sweep")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathEllipticArcAbsolute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathEllipticArcRelative")
                .param("rx", TypeExpr::Float)
                .param("ry", TypeExpr::Float)
                .param("x_axis_rotation", TypeExpr::Float)
                .param("large_arc", TypeExpr::Bool)
                .param("sweep", TypeExpr::Bool)
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_rx", e_var("rx")),
                    s_assign("_u_ry", e_var("ry")),
                    s_assign("_u_x_axis_rotation", e_var("x_axis_rotation")),
                    s_assign("_u_large_arc", e_var("large_arc")),
                    s_assign("_u_sweep", e_var("sweep")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathEllipticArcRelative() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathFinish")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathFinish() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathLineToAbsolute")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathLineToAbsolute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathLineToHorizontalAbsolute")
                .param("x", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathLineToHorizontalAbsolute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathLineToHorizontalRelative")
                .param("x", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathLineToHorizontalRelative() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathLineToRelative")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathLineToRelative() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathLineToVerticalAbsolute")
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathLineToVerticalAbsolute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathLineToVerticalRelative")
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathLineToVerticalRelative() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathMoveToAbsolute")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathMoveToAbsolute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathMoveToRelative")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathMoveToRelative() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pathStart")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pathStart() is not supported in elephc")])),
                ]),
        )
        .method(
            method("polyline")
                .param("coordinates", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_coordinates", e_var("coordinates")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::polyline() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pop")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pop() is not supported in elephc")])),
                ]),
        )
        .method(
            method("popClipPath")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::popClipPath() is not supported in elephc")])),
                ]),
        )
        .method(
            method("popDefs")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::popDefs() is not supported in elephc")])),
                ]),
        )
        .method(
            method("popPattern")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::popPattern() is not supported in elephc")])),
                ]),
        )
        .method(
            method("push")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::push() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pushClipPath")
                .param("clip_mask_id", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_clip_mask_id", e_var("clip_mask_id")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pushClipPath() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pushDefs")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pushDefs() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pushPattern")
                .param("pattern_id", TypeExpr::Str)
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .param("width", TypeExpr::Float)
                .param("height", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_pattern_id", e_var("pattern_id")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::pushPattern() is not supported in elephc")])),
                ]),
        )
        .method(
            method("render")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::render() is not supported in elephc")])),
                ]),
        )
        .method(
            method("resetVectorGraphics")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::resetVectorGraphics() is not supported in elephc")])),
                ]),
        )
        .method(
            method("rotate")
                .param("degrees", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_degrees", e_var("degrees")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::rotate() is not supported in elephc")])),
                ]),
        )
        .method(
            method("roundRectangle")
                .param("top_left_x", TypeExpr::Float)
                .param("top_left_y", TypeExpr::Float)
                .param("bottom_right_x", TypeExpr::Float)
                .param("bottom_right_y", TypeExpr::Float)
                .param("rounding_x", TypeExpr::Float)
                .param("rounding_y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_top_left_x", e_var("top_left_x")),
                    s_assign("_u_top_left_y", e_var("top_left_y")),
                    s_assign("_u_bottom_right_x", e_var("bottom_right_x")),
                    s_assign("_u_bottom_right_y", e_var("bottom_right_y")),
                    s_assign("_u_rounding_x", e_var("rounding_x")),
                    s_assign("_u_rounding_y", e_var("rounding_y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::roundRectangle() is not supported in elephc")])),
                ]),
        )
        .method(
            method("scale")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::scale() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setClipPath")
                .param("clip_mask", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_clip_mask", e_var("clip_mask")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setClipPath() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setClipRule")
                .param("fillrule", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_fillrule", e_var("fillrule")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setClipRule() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setClipUnits")
                .param("pathunits", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_pathunits", e_var("pathunits")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setClipUnits() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFillAlpha")
                .param("alpha", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_alpha", e_var("alpha")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFillAlpha() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFillOpacity")
                .param("opacity", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_opacity", e_var("opacity")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFillOpacity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFillPatternURL")
                .param("fill_url", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_fill_url", e_var("fill_url")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFillPatternURL() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFillRule")
                .param("fillrule", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_fillrule", e_var("fillrule")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFillRule() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFont")
                .param("font_name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_font_name", e_var("font_name")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFont() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFontFamily")
                .param("font_family", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_font_family", e_var("font_family")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFontFamily() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFontSize")
                .param("point_size", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_point_size", e_var("point_size")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFontSize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFontStretch")
                .param("stretch", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_stretch", e_var("stretch")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFontStretch() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFontStyle")
                .param("style", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_style", e_var("style")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFontStyle() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFontWeight")
                .param("weight", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_weight", e_var("weight")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setFontWeight() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setGravity")
                .param("gravity", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_gravity", e_var("gravity")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setGravity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setResolution")
                .param("resolution_x", TypeExpr::Float)
                .param("resolution_y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_resolution_x", e_var("resolution_x")),
                    s_assign("_u_resolution_y", e_var("resolution_y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setResolution() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setStrokeAlpha")
                .param("alpha", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_alpha", e_var("alpha")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setStrokeAlpha() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setStrokeAntialias")
                .param("enabled", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_enabled", e_var("enabled")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setStrokeAntialias() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setStrokeDashArray")
                .param("dashes", t_nullable(t_array()))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_dashes", e_var("dashes")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setStrokeDashArray() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setStrokeDashOffset")
                .param("dash_offset", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_dash_offset", e_var("dash_offset")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setStrokeDashOffset() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setStrokeLineCap")
                .param("linecap", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_linecap", e_var("linecap")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setStrokeLineCap() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setStrokeLineJoin")
                .param("linejoin", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_linejoin", e_var("linejoin")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setStrokeLineJoin() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setStrokeMiterLimit")
                .param("miterlimit", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_miterlimit", e_var("miterlimit")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setStrokeMiterLimit() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setStrokeOpacity")
                .param("opacity", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_opacity", e_var("opacity")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setStrokeOpacity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setStrokePatternURL")
                .param("stroke_url", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_stroke_url", e_var("stroke_url")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setStrokePatternURL() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setTextAlignment")
                .param("align", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_align", e_var("align")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setTextAlignment() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setTextAntialias")
                .param("antialias", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_antialias", e_var("antialias")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setTextAntialias() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setTextDecoration")
                .param("decoration", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_decoration", e_var("decoration")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setTextDecoration() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setTextEncoding")
                .param("encoding", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_encoding", e_var("encoding")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setTextEncoding() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setTextInterlineSpacing")
                .param("spacing", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_spacing", e_var("spacing")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setTextInterlineSpacing() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setTextInterwordSpacing")
                .param("spacing", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_spacing", e_var("spacing")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setTextInterwordSpacing() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setTextKerning")
                .param("kerning", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_kerning", e_var("kerning")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setTextKerning() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setTextUnderColor")
                .param("under_color", t_union(vec![t_class("ImagickPixel"), TypeExpr::Str]))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_under_color", e_var("under_color")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setTextUnderColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setVectorGraphics")
                .param("xml", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_xml", e_var("xml")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setVectorGraphics() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setViewbox")
                .param("left_x", TypeExpr::Int)
                .param("top_y", TypeExpr::Int)
                .param("right_x", TypeExpr::Int)
                .param("bottom_y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_left_x", e_var("left_x")),
                    s_assign("_u_top_y", e_var("top_y")),
                    s_assign("_u_right_x", e_var("right_x")),
                    s_assign("_u_bottom_y", e_var("bottom_y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::setViewbox() is not supported in elephc")])),
                ]),
        )
        .method(
            method("skewX")
                .param("degrees", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_degrees", e_var("degrees")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::skewX() is not supported in elephc")])),
                ]),
        )
        .method(
            method("skewY")
                .param("degrees", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_degrees", e_var("degrees")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::skewY() is not supported in elephc")])),
                ]),
        )
        .method(
            method("translate")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickDrawException", vec![e_str("ImagickDraw::translate() is not supported in elephc")])),
                ]),
        )
        // --- end auto-generated API-surface stubs ---
        .build()
}

/// `Imagick` — transcribed from the PHP form.
fn decl_class_imagick() -> Stmt {
    class("Imagick")
        .implements("Iterator")
        .implements("Countable")
        .constant("FILTER_UNDEFINED", e_int(0))
        .constant("FILTER_POINT", e_int(1))
        .constant("FILTER_BOX", e_int(2))
        .constant("FILTER_TRIANGLE", e_int(3))
        .constant("FILTER_HERMITE", e_int(4))
        .constant("FILTER_HANNING", e_int(5))
        .constant("FILTER_HAMMING", e_int(6))
        .constant("FILTER_BLACKMAN", e_int(7))
        .constant("FILTER_GAUSSIAN", e_int(8))
        .constant("FILTER_QUADRATIC", e_int(9))
        .constant("FILTER_CUBIC", e_int(10))
        .constant("FILTER_CATROM", e_int(11))
        .constant("FILTER_MITCHELL", e_int(12))
        .constant("FILTER_LANCZOS", e_int(22))
        .constant("FILTER_SINC", e_int(19))
        .constant("COMPOSITE_DEFAULT", e_int(40))
        .constant("COMPOSITE_OVER", e_int(40))
        .constant("COMPOSITE_COPY", e_int(42))
        .constant("COMPOSITE_MULTIPLY", e_int(30))
        .constant("COMPOSITE_SCREEN", e_int(46))
        .constant("COMPOSITE_ADD", e_int(7))
        .constant("CHANNEL_RED", e_int(1))
        .constant("CHANNEL_GREEN", e_int(2))
        .constant("CHANNEL_BLUE", e_int(4))
        .constant("CHANNEL_ALPHA", e_int(8))
        .constant("CHANNEL_OPACITY", e_int(8))
        .constant("CHANNEL_ALL", e_int(134217727))
        .constant("CHANNEL_DEFAULT", e_int(134217719))
        .constant("COLOR_BLACK", e_int(0))
        .constant("COLOR_BLUE", e_int(1))
        .constant("COLOR_GREEN", e_int(3))
        .constant("COLOR_RED", e_int(4))
        .constant("COLOR_OPACITY", e_int(7))
        .constant("COLOR_ALPHA", e_int(8))
        .constant("IMGTYPE_UNDEFINED", e_int(0))
        .constant("IMGTYPE_GRAYSCALE", e_int(2))
        .constant("IMGTYPE_PALETTE", e_int(3))
        .constant("IMGTYPE_TRUECOLOR", e_int(6))
        .constant("ORIENTATION_UNDEFINED", e_int(0))
        .constant("ORIENTATION_TOPLEFT", e_int(1))
        .private_prop("wand", TypeExpr::Int, Some(e_int(0)))
        .private_prop("_iterPos", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param_default("files", t_nullable(TypeExpr::Str), e_null())
                .body(vec![
                    s_prop_assign(e_this(), "wand", e_call("elephc_imagick_new", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("files"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("files"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_expr(e_method_call(e_this(), "readImage", vec![e_cast(CastType::String, e_var("files"))])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("_wandHandle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("wand")),
                ]),
        )
        .method(
            method("readImage")
                .param("filename", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_read_file", vec![e_this_prop("wand"), e_var("filename")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_binop(e_binop(e_str("Imagick::readImage(): unable to read '"), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("'"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("readImageBlob")
                .param("image", TypeExpr::Str)
                .param_default("filename", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_name", e_var("filename")),
                    s_assign("_len", e_call("strlen", vec![e_var("image")])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::LtEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::readImageBlob(): empty blob")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_buf", e_call("elephc_img_stage_ptr", vec![e_var("_len")])),
                    s_if(
                        e_call("__elephc_ptr_is_null", vec![e_var("_buf")]),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::readImageBlob(): allocation failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_ptr_write_string", vec![e_var("_buf"), e_var("image")])),
                    s_if(
                        e_binop(e_call("elephc_imagick_read_blob", vec![e_this_prop("wand"), e_var("_len")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::readImageBlob(): unrecognized image data")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("newImage")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .param_untyped("background")
                .param_default("format", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_bg", e_call("_imagick_norm_color", vec![e_var("background")])),
                    s_assign("_fmt", e_ternary(e_binop(e_var("format"), BinOp::StrictEq, e_str("")), e_int(0), e_call("_imagick_fmt_to_code", vec![e_var("format")]))),
                    s_if(
                        e_binop(e_call("elephc_imagick_new_image", vec![e_this_prop("wand"), e_var("columns"), e_var("rows"), e_var("_bg"), e_var("_fmt")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::newImage(): invalid dimensions")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("addImage")
                .param("source", t_class("Imagick"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_add_image", vec![e_this_prop("wand"), e_method_call(e_var("source"), "_wandHandle", vec![])]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::addImage(): no source image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("writeImage")
                .param_default("filename", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_var("filename"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("filename"), BinOp::StrictEq, e_str(""))),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::writeImage(): no filename given")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_path", e_cast(CastType::String, e_var("filename"))),
                    s_assign("_fmt", e_call("_imagick_fmt_from_path", vec![e_var("_path")])),
                    s_if(
                        e_binop(e_call("elephc_imagick_write_file", vec![e_this_prop("wand"), e_var("_path"), e_var("_fmt")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_binop(e_binop(e_str("Imagick::writeImage(): unable to write '"), BinOp::Concat, e_var("_path")), BinOp::Concat, e_str("'"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("writeImages")
                .param("filename", TypeExpr::Str)
                .param("adjoin", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_adjoin", e_var("adjoin")),
                    s_return(e_method_call(e_this(), "writeImage", vec![e_var("filename")])),
                ]),
        )
        .method(
            method("getImageBlob")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_len", e_call("elephc_imagick_get_blob", vec![e_this_prop("wand"), e_int(0)])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageBlob(): no image or encode failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_bytes", e_call("__elephc_ptr_read_string", vec![e_call("elephc_img_encoded_ptr", vec![]), e_var("_len")])),
                    s_expr(e_call("elephc_img_encoded_clear", vec![])),
                    s_return(e_var("_bytes")),
                ]),
        )
        .method(
            method("getImagesBlob")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_method_call(e_this(), "getImageBlob", vec![])),
                ]),
        )
        .method(
            method("setImageFormat")
                .param("format", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_code", e_call("_imagick_fmt_to_code", vec![e_var("format")])),
                    s_if(
                        e_binop(e_var("_code"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_binop(e_binop(e_str("Imagick::setImageFormat(): unsupported format '"), BinOp::Concat, e_var("format")), BinOp::Concat, e_str("'"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_imagick_set_format", vec![e_this_prop("wand"), e_var("_code")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("getImageFormat")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("_imagick_code_to_fmt", vec![e_call("elephc_imagick_get_format", vec![e_this_prop("wand")])])),
                ]),
        )
        .method(
            method("setFormat")
                .param("format", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "setImageFormat", vec![e_var("format")])),
                ]),
        )
        .method(
            method("getFormat")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_method_call(e_this(), "getImageFormat", vec![])),
                ]),
        )
        .method(
            method("setImageCompressionQuality")
                .param("quality", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_imagick_set_quality", vec![e_this_prop("wand"), e_var("quality")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("getImageCompressionQuality")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("_q", e_call("elephc_imagick_get_quality", vec![e_this_prop("wand")])),
                    s_return(e_ternary(e_binop(e_var("_q"), BinOp::Lt, e_int(0)), e_int(0), e_var("_q"))),
                ]),
        )
        .method(
            method("setCompressionQuality")
                .param("quality", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "setImageCompressionQuality", vec![e_var("quality")])),
                ]),
        )
        .method(
            method("getImageWidth")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_cur_width", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("getImageHeight")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_cur_height", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("getImageGeometry")
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("width"), e_method_call(e_this(), "getImageWidth", vec![])), (e_str("height"), e_method_call(e_this(), "getImageHeight", vec![]))])),
                ]),
        )
        .method(
            method("_bestfit")
                .private()
                .param("cols", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .returns(t_array())
                .body(vec![
                    s_assign("_ow", e_method_call(e_this(), "getImageWidth", vec![])),
                    s_assign("_oh", e_method_call(e_this(), "getImageHeight", vec![])),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("_ow"), BinOp::LtEq, e_int(0)), BinOp::Or, e_binop(e_var("_oh"), BinOp::LtEq, e_int(0))), BinOp::Or, e_binop(e_var("cols"), BinOp::LtEq, e_int(0))), BinOp::Or, e_binop(e_var("rows"), BinOp::LtEq, e_int(0))),
                        vec![
                            s_return(e_array(vec![e_var("cols"), e_var("rows")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_rw", e_binop(e_var("cols"), BinOp::Div, e_var("_ow"))),
                    s_assign("_rh", e_binop(e_var("rows"), BinOp::Div, e_var("_oh"))),
                    s_assign("_ratio", e_ternary(e_binop(e_var("_rw"), BinOp::Lt, e_var("_rh")), e_var("_rw"), e_var("_rh"))),
                    s_return(e_array(vec![e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("_ow"), BinOp::Mul, e_var("_ratio"))])), e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("_oh"), BinOp::Mul, e_var("_ratio"))]))])),
                ]),
        )
        .method(
            method("resizeImage")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .param("filter", TypeExpr::Int)
                .param("blur", TypeExpr::Float)
                .param_default("bestfit", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_filter", e_var("filter")),
                    s_assign("_u_blur", e_var("blur")),
                    s_assign("_d", e_ternary(e_var("bestfit"), e_method_call(e_this(), "_bestfit", vec![e_var("columns"), e_var("rows")]), e_array(vec![e_var("columns"), e_var("rows")]))),
                    s_if(
                        e_binop(e_call("elephc_imagick_resize", vec![e_this_prop("wand"), e_index(e_var("_d"), e_int(0)), e_index(e_var("_d"), e_int(1))]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::resizeImage(): resize failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("scaleImage")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .param_default("bestfit", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_d", e_ternary(e_var("bestfit"), e_method_call(e_this(), "_bestfit", vec![e_var("columns"), e_var("rows")]), e_array(vec![e_var("columns"), e_var("rows")]))),
                    s_if(
                        e_binop(e_call("elephc_imagick_scale", vec![e_this_prop("wand"), e_index(e_var("_d"), e_int(0)), e_index(e_var("_d"), e_int(1))]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::scaleImage(): scale failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("thumbnailImage")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .param_default("bestfit", TypeExpr::Bool, e_bool(false))
                .param_default("fill", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_fill", e_var("fill")),
                    s_assign("_ow", e_method_call(e_this(), "getImageWidth", vec![])),
                    s_assign("_oh", e_method_call(e_this(), "getImageHeight", vec![])),
                    s_assign("_w", e_var("columns")),
                    s_assign("_h", e_var("rows")),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("columns"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_var("rows"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_var("_oh"), BinOp::Gt, e_int(0))),
                        vec![
                            s_assign("_w", e_cast(CastType::Int, e_call("round", vec![e_binop(e_binop(e_var("_ow"), BinOp::Mul, e_var("rows")), BinOp::Div, e_var("_oh"))]))),
                        ],
                        vec![
                        (e_binop(e_binop(e_binop(e_var("rows"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_var("columns"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_var("_ow"), BinOp::Gt, e_int(0))), vec![
                            s_assign("_h", e_cast(CastType::Int, e_call("round", vec![e_binop(e_binop(e_var("_oh"), BinOp::Mul, e_var("columns")), BinOp::Div, e_var("_ow"))]))),
                        ]),
                        (e_binop(e_binop(e_var("bestfit"), BinOp::And, e_binop(e_var("columns"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_var("rows"), BinOp::Gt, e_int(0))), vec![
                            s_assign("_d", e_method_call(e_this(), "_bestfit", vec![e_var("columns"), e_var("rows")])),
                            s_assign("_w", e_index(e_var("_d"), e_int(0))),
                            s_assign("_h", e_index(e_var("_d"), e_int(1))),
                        ]),
                    ],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_w"), BinOp::Lt, e_int(1)),
                        vec![
                            s_assign("_w", e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_h"), BinOp::Lt, e_int(1)),
                        vec![
                            s_assign("_h", e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_imagick_resize", vec![e_this_prop("wand"), e_var("_w"), e_var("_h")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::thumbnailImage(): failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("cropImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_crop", vec![e_this_prop("wand"), e_var("width"), e_var("height"), e_var("x"), e_var("y")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::cropImage(): invalid crop region")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("rotateImage")
                .param_untyped("background")
                .param("degrees", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_bg", e_call("_imagick_norm_color", vec![e_var("background")])),
                    s_assign("_mdeg", e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("degrees"), BinOp::Mul, e_int(1000))]))),
                    s_if(
                        e_binop(e_call("elephc_imagick_rotate", vec![e_this_prop("wand"), e_var("_mdeg"), e_var("_bg")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::rotateImage(): rotate failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("flipImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_flip", vec![e_this_prop("wand")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::flipImage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("flopImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_flop", vec![e_this_prop("wand")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::flopImage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("blurImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_sig", e_ternary(e_binop(e_var("sigma"), BinOp::Gt, e_float(0.0)), e_var("sigma"), e_var("radius"))),
                    s_if(
                        e_binop(e_call("elephc_imagick_blur", vec![e_this_prop("wand"), e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("_sig"), BinOp::Mul, e_int(1000))]))]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::blurImage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("gaussianBlurImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "blurImage", vec![e_var("radius"), e_var("sigma")])),
                ]),
        )
        .method(
            method("negateImage")
                .param_default("gray", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_negate", vec![e_this_prop("wand"), e_ternary(e_var("gray"), e_int(1), e_int(0))]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::negateImage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("modulateImage")
                .param_untyped("brightness")
                .param_untyped("saturation")
                .param_untyped("hue")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_b", e_cast(CastType::Int, e_call("round", vec![e_var("brightness")]))),
                    s_assign("_s", e_cast(CastType::Int, e_call("round", vec![e_var("saturation")]))),
                    s_assign("_h", e_cast(CastType::Int, e_call("round", vec![e_var("hue")]))),
                    s_if(
                        e_binop(e_call("elephc_imagick_modulate", vec![e_this_prop("wand"), e_var("_b"), e_var("_s"), e_var("_h")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::modulateImage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("sharpenImage")
                .param_untyped("radius")
                .param_untyped("sigma")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_r", e_cast(CastType::Int, e_call("round", vec![e_binop(e_cast(CastType::Float, e_var("radius")), BinOp::Mul, e_int(1000))]))),
                    s_assign("_s", e_cast(CastType::Int, e_call("round", vec![e_binop(e_cast(CastType::Float, e_var("sigma")), BinOp::Mul, e_int(1000))]))),
                    s_if(
                        e_binop(e_call("elephc_imagick_sharpen", vec![e_this_prop("wand"), e_var("_r"), e_var("_s")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::sharpenImage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("compositeImage")
                .param("composite", t_class("Imagick"))
                .param("composite_op", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_rc", e_call("elephc_imagick_composite", vec![e_this_prop("wand"), e_method_call(e_var("composite"), "_wandHandle", vec![]), e_var("composite_op"), e_var("x"), e_var("y")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::StrictEq, e_neg(e_int(2))),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_binop(e_binop(e_str("Imagick::compositeImage(): composite operator "), BinOp::Concat, e_var("composite_op")), BinOp::Concat, e_str(" is not supported in elephc"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::compositeImage(): composite failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("drawImage")
                .param("draw", t_class("ImagickDraw"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_draw", vec![e_this_prop("wand"), e_method_call(e_var("draw"), "_imagickHandle", vec![])]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::drawImage(): draw failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("convolveImage")
                .param("kernel", t_class("ImagickKernel"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_method_call(e_var("kernel"), "_size", vec![]), BinOp::StrictNotEq, e_int(3)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::convolveImage(): only 3x3 kernels are supported in elephc")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_img_fbuf_reset", vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_int(9))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_expr(e_call("elephc_img_fbuf_push", vec![e_cast(CastType::Int, e_call("round", vec![e_binop(e_method_call(e_var("kernel"), "_at", vec![e_var("_i")]), BinOp::Mul, e_int(65536))]))])),
                    ]),
                    s_assign("_div", e_cast(CastType::Int, e_call("round", vec![e_binop(e_method_call(e_var("kernel"), "_divisor", vec![]), BinOp::Mul, e_int(65536))]))),
                    s_if(
                        e_binop(e_call("elephc_imagick_convolve", vec![e_this_prop("wand"), e_var("_div"), e_int(0)]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::convolveImage(): convolution failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("getImagePixelColor")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(t_class("ImagickPixel"))
                .body(vec![
                    s_assign("_packed", e_call("elephc_imagick_pixel_color", vec![e_this_prop("wand"), e_var("x"), e_var("y")])),
                    s_if(
                        e_binop(e_var("_packed"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ImagickException", vec![e_str("Imagick::getImagePixelColor(): coordinate out of range")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("_imagick_pixel_from_int", vec![e_var("_packed")])),
                ]),
        )
        .method(
            method("setImageBackgroundColor")
                .param_untyped("background")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_imagick_fill", vec![e_this_prop("wand"), e_call("_imagick_norm_color", vec![e_var("background")])])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("getPixelIterator")
                .returns(t_class("ImagickPixelIterator"))
                .body(vec![
                    s_return(e_new("ImagickPixelIterator", vec![e_this()])),
                ]),
        )
        .method(
            method("getNumberImages")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_count", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("getImageIndex")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_get_index", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("setImageIndex")
                .param("index", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_imagick_set_index", vec![e_this_prop("wand"), e_var("index")]), BinOp::StrictEq, e_int(0))),
                ]),
        )
        .method(
            method("getIteratorIndex")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_get_index", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("setIteratorIndex")
                .param("index", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_imagick_set_index", vec![e_this_prop("wand"), e_var("index")]), BinOp::StrictEq, e_int(0))),
                ]),
        )
        .method(
            method("nextImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_imagick_next", vec![e_this_prop("wand")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("previousImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_imagick_previous", vec![e_this_prop("wand")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("setFirstIterator")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_imagick_first", vec![e_this_prop("wand")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("setLastIterator")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_imagick_last", vec![e_this_prop("wand")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("count")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_count", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("rewind")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "_iterPos", e_int(0)),
                    s_expr(e_call("elephc_imagick_set_index", vec![e_this_prop("wand"), e_int(0)])),
                ]),
        )
        .method(
            method("valid")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_this_prop("_iterPos"), BinOp::Lt, e_call("elephc_imagick_count", vec![e_this_prop("wand")]))),
                ]),
        )
        .method(
            method("current")
                .returns(t_mixed())
                .body(vec![
                    s_expr(e_call("elephc_imagick_set_index", vec![e_this_prop("wand"), e_this_prop("_iterPos")])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("key")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_this_prop("_iterPos")),
                ]),
        )
        .method(
            method("next")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "_iterPos", e_binop(e_this_prop("_iterPos"), BinOp::Add, e_int(1))),
                ]),
        )
        .method(
            method("clear")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_imagick_clear", vec![e_this_prop("wand")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("destroy")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_imagick_destroy", vec![e_this_prop("wand")])),
                    s_prop_assign(e_this(), "wand", e_int(0)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_expr(e_call("elephc_imagick_destroy", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("queryFormats")
                .static_()
                .param_default("pattern", TypeExpr::Str, e_str("*"))
                .returns(t_array())
                .body(vec![
                    s_assign("_u_pat", e_var("pattern")),
                    s_return(e_array(vec![e_str("BMP"), e_str("GIF"), e_str("JPEG"), e_str("PNG"), e_str("WEBP")])),
                ]),
        )
        .method(
            method("distortImage")
                .param("method", TypeExpr::Int)
                .param("arguments", t_array())
                .param("bestfit", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_m", e_var("method")),
                    s_assign("_u_a", e_var("arguments")),
                    s_assign("_u_b", e_var("bestfit")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::distortImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("liquidRescaleImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("delta_x", TypeExpr::Float)
                .param("rigidity", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_w", e_var("width")),
                    s_assign("_u_h", e_var("height")),
                    s_assign("_u_dx", e_var("delta_x")),
                    s_assign("_u_r", e_var("rigidity")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::liquidRescaleImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("fxImage")
                .param("expression", TypeExpr::Str)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_e", e_var("expression")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::fxImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("annotateImage")
                .param("draw", t_class("ImagickDraw"))
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .param("angle", TypeExpr::Float)
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_d", e_var("draw")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_a", e_var("angle")),
                    s_assign("_u_t", e_var("text")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::annotateImage() requires FreeType text, which is not supported in elephc")])),
                ]),
        )
        .method(
            method("waveImage")
                .param("amplitude", TypeExpr::Float)
                .param("length", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_a", e_var("amplitude")),
                    s_assign("_u_l", e_var("length")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::waveImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("swirlImage")
                .param("degrees", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_d", e_var("degrees")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::swirlImage() is not supported in elephc")])),
                ]),
        )
        // --- begin auto-generated API-surface throwing stubs (do not edit; regen via crates/elephc-image/tools/gen_image_api_stubs.py) ---
        .method(
            method("adaptiveBlurImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::adaptiveBlurImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("adaptiveResizeImage")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .param_default("bestfit", TypeExpr::Bool, e_bool(false))
                .param_default("legacy", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_assign("_u_bestfit", e_var("bestfit")),
                    s_assign("_u_legacy", e_var("legacy")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::adaptiveResizeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("adaptiveSharpenImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::adaptiveSharpenImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("adaptiveThresholdImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("offset", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_offset", e_var("offset")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::adaptiveThresholdImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("addNoiseImage")
                .param("noise_type", TypeExpr::Int)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_noise_type", e_var("noise_type")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::addNoiseImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("affineTransformImage")
                .param("matrix", t_class("ImagickDraw"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_matrix", e_var("matrix")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::affineTransformImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("animateImages")
                .param("x_server", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x_server", e_var("x_server")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::animateImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("appendImages")
                .param("stack", TypeExpr::Bool)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_stack", e_var("stack")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::appendImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("autoLevelImage")
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::autoLevelImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("averageImages")
                .returns(t_class("Imagick"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::averageImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("blackThresholdImage")
                .param_untyped("threshold")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_threshold", e_var("threshold")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::blackThresholdImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("blueShiftImage")
                .param_default("factor", TypeExpr::Float, e_float(1.5))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_factor", e_var("factor")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::blueShiftImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("borderImage")
                .param_untyped("bordercolor")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_bordercolor", e_var("bordercolor")),
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::borderImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("brightnessContrastImage")
                .param("brightness", TypeExpr::Float)
                .param("contrast", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_brightness", e_var("brightness")),
                    s_assign("_u_contrast", e_var("contrast")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::brightnessContrastImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("charcoalImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::charcoalImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("chopImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::chopImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("clampImage")
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::clampImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("clipImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::clipImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("clipImagePath")
                .param("pathname", TypeExpr::Str)
                .param("inside", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_u_pathname", e_var("pathname")),
                    s_assign("_u_inside", e_var("inside")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::clipImagePath() is not supported in elephc")])),
                ]),
        )
        .method(
            method("clipPathImage")
                .param("pathname", TypeExpr::Str)
                .param("inside", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_pathname", e_var("pathname")),
                    s_assign("_u_inside", e_var("inside")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::clipPathImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("clone")
                .returns(t_class("Imagick"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::clone() is not supported in elephc")])),
                ]),
        )
        .method(
            method("clutImage")
                .param("lookup_table", t_class("Imagick"))
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_lookup_table", e_var("lookup_table")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::clutImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("coalesceImages")
                .returns(t_class("Imagick"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::coalesceImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("colorFloodfillImage")
                .param_untyped("fill")
                .param("fuzz", TypeExpr::Float)
                .param_untyped("bordercolor")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_fill", e_var("fill")),
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_assign("_u_bordercolor", e_var("bordercolor")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::colorFloodfillImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("colorizeImage")
                .param_untyped("colorize")
                .param_untyped("opacity")
                .param_default("legacy", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_colorize", e_var("colorize")),
                    s_assign("_u_opacity", e_var("opacity")),
                    s_assign("_u_legacy", e_var("legacy")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::colorizeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("colorMatrixImage")
                .param("color_matrix", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_color_matrix", e_var("color_matrix")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::colorMatrixImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("combineImages")
                .param("channelType", TypeExpr::Int)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_channelType", e_var("channelType")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::combineImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("commentImage")
                .param("comment", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_comment", e_var("comment")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::commentImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("compareImageChannels")
                .param("image", t_class("Imagick"))
                .param("channelType", TypeExpr::Int)
                .param("metricType", TypeExpr::Int)
                .returns(t_array())
                .body(vec![
                    s_assign("_u_image", e_var("image")),
                    s_assign("_u_channelType", e_var("channelType")),
                    s_assign("_u_metricType", e_var("metricType")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::compareImageChannels() is not supported in elephc")])),
                ]),
        )
        .method(
            method("compareImageLayers")
                .param("method", TypeExpr::Int)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_method", e_var("method")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::compareImageLayers() is not supported in elephc")])),
                ]),
        )
        .method(
            method("compareImages")
                .param("compare", t_class("Imagick"))
                .param("metric", TypeExpr::Int)
                .returns(t_array())
                .body(vec![
                    s_assign("_u_compare", e_var("compare")),
                    s_assign("_u_metric", e_var("metric")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::compareImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("contrastImage")
                .param("sharpen", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_sharpen", e_var("sharpen")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::contrastImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("contrastStretchImage")
                .param("black_point", TypeExpr::Float)
                .param("white_point", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_black_point", e_var("black_point")),
                    s_assign("_u_white_point", e_var("white_point")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::contrastStretchImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("cropThumbnailImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param_default("legacy", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_legacy", e_var("legacy")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::cropThumbnailImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("cycleColormapImage")
                .param("displace", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_displace", e_var("displace")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::cycleColormapImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("decipherImage")
                .param("passphrase", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_passphrase", e_var("passphrase")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::decipherImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("deconstructImages")
                .returns(t_class("Imagick"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::deconstructImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("deleteImageArtifact")
                .param("artifact", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_artifact", e_var("artifact")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::deleteImageArtifact() is not supported in elephc")])),
                ]),
        )
        .method(
            method("deleteImageProperty")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::deleteImageProperty() is not supported in elephc")])),
                ]),
        )
        .method(
            method("deskewImage")
                .param("threshold", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_threshold", e_var("threshold")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::deskewImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("despeckleImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::despeckleImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("displayImage")
                .param("servername", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_servername", e_var("servername")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::displayImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("displayImages")
                .param("servername", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_servername", e_var("servername")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::displayImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("edgeImage")
                .param("radius", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::edgeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("embossImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::embossImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("encipherImage")
                .param("passphrase", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_passphrase", e_var("passphrase")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::encipherImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("enhanceImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::enhanceImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("equalizeImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::equalizeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("evaluateImage")
                .param("op", TypeExpr::Int)
                .param("constant", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_op", e_var("op")),
                    s_assign("_u_constant", e_var("constant")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::evaluateImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("exportImagePixels")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("map", TypeExpr::Str)
                .param("sTORAGE", TypeExpr::Int)
                .returns(t_array())
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_map", e_var("map")),
                    s_assign("_u_sTORAGE", e_var("sTORAGE")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::exportImagePixels() is not supported in elephc")])),
                ]),
        )
        .method(
            method("extentImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::extentImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("filter")
                .param("imagickKernel", t_class("ImagickKernel"))
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_imagickKernel", e_var("imagickKernel")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::filter() is not supported in elephc")])),
                ]),
        )
        .method(
            method("flattenImages")
                .returns(t_class("Imagick"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::flattenImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("floodFillPaintImage")
                .param_untyped("fill")
                .param("fuzz", TypeExpr::Float)
                .param_untyped("target")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .param("invert", TypeExpr::Bool)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_fill", e_var("fill")),
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_assign("_u_target", e_var("target")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_invert", e_var("invert")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::floodFillPaintImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("forwardFourierTransformimage")
                .param("magnitude", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_magnitude", e_var("magnitude")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::forwardFourierTransformimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("frameImage")
                .param_untyped("matte_color")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("inner_bevel", TypeExpr::Int)
                .param("outer_bevel", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_matte_color", e_var("matte_color")),
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_inner_bevel", e_var("inner_bevel")),
                    s_assign("_u_outer_bevel", e_var("outer_bevel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::frameImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("functionImage")
                .param("function", TypeExpr::Int)
                .param("arguments", t_array())
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_function", e_var("function")),
                    s_assign("_u_arguments", e_var("arguments")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::functionImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("gammaImage")
                .param("gamma", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_gamma", e_var("gamma")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::gammaImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getColorspace")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getColorspace() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getCompression")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getCompression() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getCompressionQuality")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getCompressionQuality() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getCopyright")
                .static_()
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getCopyright() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFilename")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getFilename() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getFont")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getFont() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getGravity")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getGravity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getHomeURL")
                .static_()
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getHomeURL() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImage")
                .returns(t_class("Imagick"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageAlphaChannel")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageAlphaChannel() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageArtifact")
                .param("artifact", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_artifact", e_var("artifact")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageArtifact() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageAttribute")
                .param("key", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_key", e_var("key")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageAttribute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageBackgroundColor")
                .returns(t_class("ImagickPixel"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageBackgroundColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageBluePrimary")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageBluePrimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageBorderColor")
                .returns(t_class("ImagickPixel"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageBorderColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageChannelDepth")
                .param("channel", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageChannelDepth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageChannelDistortion")
                .param("reference", t_class("Imagick"))
                .param("channel", TypeExpr::Int)
                .param("metric", TypeExpr::Int)
                .returns(TypeExpr::Float)
                .body(vec![
                    s_assign("_u_reference", e_var("reference")),
                    s_assign("_u_channel", e_var("channel")),
                    s_assign("_u_metric", e_var("metric")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageChannelDistortion() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageChannelDistortions")
                .param("reference", t_class("Imagick"))
                .param("metric", TypeExpr::Int)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Float)
                .body(vec![
                    s_assign("_u_reference", e_var("reference")),
                    s_assign("_u_metric", e_var("metric")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageChannelDistortions() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageChannelExtrema")
                .param("channel", TypeExpr::Int)
                .returns(t_array())
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageChannelExtrema() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageChannelKurtosis")
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(t_array())
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageChannelKurtosis() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageChannelMean")
                .param("channel", TypeExpr::Int)
                .returns(t_array())
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageChannelMean() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageChannelRange")
                .param("channel", TypeExpr::Int)
                .returns(t_array())
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageChannelRange() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageChannelStatistics")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageChannelStatistics() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageClipMask")
                .returns(t_class("Imagick"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageClipMask() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageColormapColor")
                .param("index", TypeExpr::Int)
                .returns(t_class("ImagickPixel"))
                .body(vec![
                    s_assign("_u_index", e_var("index")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageColormapColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageColors")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageColors() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageColorspace")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageColorspace() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageCompose")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageCompose() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageCompression")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageCompression() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageDelay")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageDelay() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageDepth")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageDepth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageDispose")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageDispose() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageDistortion")
                .param("reference", t_class("Imagick"))
                .param("metric", TypeExpr::Int)
                .returns(TypeExpr::Float)
                .body(vec![
                    s_assign("_u_reference", e_var("reference")),
                    s_assign("_u_metric", e_var("metric")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageDistortion() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageExtrema")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageExtrema() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageFilename")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageFilename() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageGamma")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageGamma() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageGravity")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageGravity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageGreenPrimary")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageGreenPrimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageHistogram")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageHistogram() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageInterlaceScheme")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageInterlaceScheme() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageInterpolateMethod")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageInterpolateMethod() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageIterations")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageIterations() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageLength")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageLength() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageMatte")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageMatte() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageMatteColor")
                .returns(t_class("ImagickPixel"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageMatteColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageMimeType")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageMimeType() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageOrientation")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageOrientation() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImagePage")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImagePage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageProfile")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageProfile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageProfiles")
                .param_default("pattern", TypeExpr::Str, e_str("*"))
                .param_default("include_values", TypeExpr::Bool, e_bool(true))
                .returns(t_array())
                .body(vec![
                    s_assign("_u_pattern", e_var("pattern")),
                    s_assign("_u_include_values", e_var("include_values")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageProfiles() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageProperties")
                .param_default("pattern", TypeExpr::Str, e_str("*"))
                .param_default("include_values", TypeExpr::Bool, e_bool(true))
                .returns(t_array())
                .body(vec![
                    s_assign("_u_pattern", e_var("pattern")),
                    s_assign("_u_include_values", e_var("include_values")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageProperties() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageProperty")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageProperty() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageRedPrimary")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageRedPrimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageRegion")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageRegion() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageRenderingIntent")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageRenderingIntent() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageResolution")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageResolution() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageScene")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageScene() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageSignature")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageSignature() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageSize")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageSize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageTicksPerSecond")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageTicksPerSecond() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageTotalInkDensity")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageTotalInkDensity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageType")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageType() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageUnits")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageUnits() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageVirtualPixelMethod")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageVirtualPixelMethod() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getImageWhitePoint")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getImageWhitePoint() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getInterlaceScheme")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getInterlaceScheme() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getOption")
                .param("key", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_key", e_var("key")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getOption() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getPackageName")
                .static_()
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getPackageName() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getPage")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getPage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getPixelRegionIterator")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .returns(t_class("ImagickPixelIterator"))
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getPixelRegionIterator() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getPointSize")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getPointSize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getQuantum")
                .static_()
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getQuantum() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getQuantumDepth")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getQuantumDepth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getQuantumRange")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getQuantumRange() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getRegistry")
                .static_()
                .param("key", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_key", e_var("key")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getRegistry() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getReleaseDate")
                .static_()
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getReleaseDate() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getResource")
                .static_()
                .param("type", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("_u_type", e_var("type")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getResource() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getResourceLimit")
                .static_()
                .param("type", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("_u_type", e_var("type")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getResourceLimit() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getSamplingFactors")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getSamplingFactors() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getSize")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getSize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getSizeOffset")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getSizeOffset() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getVersion")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::getVersion() is not supported in elephc")])),
                ]),
        )
        .method(
            method("haldClutImage")
                .param("clut", t_class("Imagick"))
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_clut", e_var("clut")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::haldClutImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("hasNextImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::hasNextImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("hasPreviousImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::hasPreviousImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("identifyFormat")
                .param("embedText", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_embedText", e_var("embedText")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::identifyFormat() is not supported in elephc")])),
                ]),
        )
        .method(
            method("identifyImage")
                .param_default("appendRawOutput", TypeExpr::Bool, e_bool(false))
                .returns(t_array())
                .body(vec![
                    s_assign("_u_appendRawOutput", e_var("appendRawOutput")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::identifyImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("implodeImage")
                .param("radius", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::implodeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("importImagePixels")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("map", TypeExpr::Str)
                .param("storage", TypeExpr::Int)
                .param("pixels", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_map", e_var("map")),
                    s_assign("_u_storage", e_var("storage")),
                    s_assign("_u_pixels", e_var("pixels")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::importImagePixels() is not supported in elephc")])),
                ]),
        )
        .method(
            method("inverseFourierTransformImage")
                .param("complement", t_class("Imagick"))
                .param("magnitude", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_complement", e_var("complement")),
                    s_assign("_u_magnitude", e_var("magnitude")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::inverseFourierTransformImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("labelImage")
                .param("label", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_label", e_var("label")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::labelImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("levelImage")
                .param("blackPoint", TypeExpr::Float)
                .param("gamma", TypeExpr::Float)
                .param("whitePoint", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_blackPoint", e_var("blackPoint")),
                    s_assign("_u_gamma", e_var("gamma")),
                    s_assign("_u_whitePoint", e_var("whitePoint")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::levelImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("linearStretchImage")
                .param("blackPoint", TypeExpr::Float)
                .param("whitePoint", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_blackPoint", e_var("blackPoint")),
                    s_assign("_u_whitePoint", e_var("whitePoint")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::linearStretchImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("listRegistry")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::listRegistry() is not supported in elephc")])),
                ]),
        )
        .method(
            method("magnifyImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::magnifyImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("mapImage")
                .param("map", t_class("Imagick"))
                .param("dither", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_map", e_var("map")),
                    s_assign("_u_dither", e_var("dither")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::mapImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("matteFloodfillImage")
                .param("alpha", TypeExpr::Float)
                .param("fuzz", TypeExpr::Float)
                .param_untyped("bordercolor")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_alpha", e_var("alpha")),
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_assign("_u_bordercolor", e_var("bordercolor")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::matteFloodfillImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("medianFilterImage")
                .param("radius", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::medianFilterImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("mergeImageLayers")
                .param("layer_method", TypeExpr::Int)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_layer_method", e_var("layer_method")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::mergeImageLayers() is not supported in elephc")])),
                ]),
        )
        .method(
            method("minifyImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::minifyImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("montageImage")
                .param("draw", t_class("ImagickDraw"))
                .param("tile_geometry", TypeExpr::Str)
                .param("thumbnail_geometry", TypeExpr::Str)
                .param("mode", TypeExpr::Int)
                .param("frame", TypeExpr::Str)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_draw", e_var("draw")),
                    s_assign("_u_tile_geometry", e_var("tile_geometry")),
                    s_assign("_u_thumbnail_geometry", e_var("thumbnail_geometry")),
                    s_assign("_u_mode", e_var("mode")),
                    s_assign("_u_frame", e_var("frame")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::montageImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("morphImages")
                .param("number_frames", TypeExpr::Int)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_number_frames", e_var("number_frames")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::morphImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("morphology")
                .param("morphologyMethod", TypeExpr::Int)
                .param("iterations", TypeExpr::Int)
                .param("imagickKernel", t_class("ImagickKernel"))
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_morphologyMethod", e_var("morphologyMethod")),
                    s_assign("_u_iterations", e_var("iterations")),
                    s_assign("_u_imagickKernel", e_var("imagickKernel")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::morphology() is not supported in elephc")])),
                ]),
        )
        .method(
            method("mosaicImages")
                .returns(t_class("Imagick"))
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::mosaicImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("motionBlurImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .param("angle", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_assign("_u_angle", e_var("angle")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::motionBlurImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("newPseudoImage")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .param("pseudoString", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_assign("_u_pseudoString", e_var("pseudoString")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::newPseudoImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("normalizeImage")
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::normalizeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("oilPaintImage")
                .param("radius", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::oilPaintImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("opaquePaintImage")
                .param_untyped("target")
                .param_untyped("fill")
                .param("fuzz", TypeExpr::Float)
                .param("invert", TypeExpr::Bool)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_target", e_var("target")),
                    s_assign("_u_fill", e_var("fill")),
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_assign("_u_invert", e_var("invert")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::opaquePaintImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("optimizeImageLayers")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::optimizeImageLayers() is not supported in elephc")])),
                ]),
        )
        .method(
            method("orderedPosterizeImage")
                .param("threshold_map", TypeExpr::Str)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_threshold_map", e_var("threshold_map")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::orderedPosterizeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("paintFloodfillImage")
                .param_untyped("fill")
                .param("fuzz", TypeExpr::Float)
                .param_untyped("bordercolor")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_fill", e_var("fill")),
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_assign("_u_bordercolor", e_var("bordercolor")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::paintFloodfillImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("paintOpaqueImage")
                .param_untyped("target")
                .param_untyped("fill")
                .param("fuzz", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_target", e_var("target")),
                    s_assign("_u_fill", e_var("fill")),
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::paintOpaqueImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("paintTransparentImage")
                .param_untyped("target")
                .param("alpha", TypeExpr::Float)
                .param("fuzz", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_target", e_var("target")),
                    s_assign("_u_alpha", e_var("alpha")),
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::paintTransparentImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pingImage")
                .param("filename", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_filename", e_var("filename")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::pingImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pingImageBlob")
                .param("image", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_image", e_var("image")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::pingImageBlob() is not supported in elephc")])),
                ]),
        )
        .method(
            method("pingImageFile")
                .param_untyped("filehandle")
                .param_default("fileName", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_filehandle", e_var("filehandle")),
                    s_assign("_u_fileName", e_var("fileName")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::pingImageFile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("polaroidImage")
                .param("properties", t_class("ImagickDraw"))
                .param("angle", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_properties", e_var("properties")),
                    s_assign("_u_angle", e_var("angle")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::polaroidImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("posterizeImage")
                .param("levels", TypeExpr::Int)
                .param("dither", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_levels", e_var("levels")),
                    s_assign("_u_dither", e_var("dither")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::posterizeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("previewImages")
                .param("preview", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_preview", e_var("preview")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::previewImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("profileImage")
                .param("name", TypeExpr::Str)
                .param_default("profile", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_assign("_u_profile", e_var("profile")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::profileImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("quantizeImage")
                .param("numberColors", TypeExpr::Int)
                .param("colorspace", TypeExpr::Int)
                .param("treedepth", TypeExpr::Int)
                .param("dither", TypeExpr::Bool)
                .param("measureError", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_numberColors", e_var("numberColors")),
                    s_assign("_u_colorspace", e_var("colorspace")),
                    s_assign("_u_treedepth", e_var("treedepth")),
                    s_assign("_u_dither", e_var("dither")),
                    s_assign("_u_measureError", e_var("measureError")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::quantizeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("quantizeImages")
                .param("numberColors", TypeExpr::Int)
                .param("colorspace", TypeExpr::Int)
                .param("treedepth", TypeExpr::Int)
                .param("dither", TypeExpr::Bool)
                .param("measureError", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_numberColors", e_var("numberColors")),
                    s_assign("_u_colorspace", e_var("colorspace")),
                    s_assign("_u_treedepth", e_var("treedepth")),
                    s_assign("_u_dither", e_var("dither")),
                    s_assign("_u_measureError", e_var("measureError")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::quantizeImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("queryFontMetrics")
                .param("properties", t_class("ImagickDraw"))
                .param("text", TypeExpr::Str)
                .param_default("multiline", TypeExpr::Bool, e_bool(false))
                .returns(t_array())
                .body(vec![
                    s_assign("_u_properties", e_var("properties")),
                    s_assign("_u_text", e_var("text")),
                    s_assign("_u_multiline", e_var("multiline")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::queryFontMetrics() is not supported in elephc")])),
                ]),
        )
        .method(
            method("queryFonts")
                .static_()
                .param_default("pattern", TypeExpr::Str, e_str("*"))
                .returns(t_array())
                .body(vec![
                    s_assign("_u_pattern", e_var("pattern")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::queryFonts() is not supported in elephc")])),
                ]),
        )
        .method(
            method("radialBlurImage")
                .param("angle", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_angle", e_var("angle")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::radialBlurImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("raiseImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .param("raise", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_raise", e_var("raise")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::raiseImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("randomThresholdImage")
                .param("low", TypeExpr::Float)
                .param("high", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_low", e_var("low")),
                    s_assign("_u_high", e_var("high")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::randomThresholdImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("readImageFile")
                .param_untyped("filehandle")
                .param_default("fileName", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_filehandle", e_var("filehandle")),
                    s_assign("_u_fileName", e_var("fileName")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::readImageFile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("readImages")
                .param("filenames", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_filenames", e_var("filenames")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::readImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("recolorImage")
                .param("matrix", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_matrix", e_var("matrix")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::recolorImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("reduceNoiseImage")
                .param("radius", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::reduceNoiseImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("remapImage")
                .param("replacement", t_class("Imagick"))
                .param("dITHER", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_replacement", e_var("replacement")),
                    s_assign("_u_dITHER", e_var("dITHER")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::remapImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("removeImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::removeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("removeImageProfile")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::removeImageProfile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("render")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::render() is not supported in elephc")])),
                ]),
        )
        .method(
            method("resampleImage")
                .param("x_resolution", TypeExpr::Float)
                .param("y_resolution", TypeExpr::Float)
                .param("filter", TypeExpr::Int)
                .param("blur", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x_resolution", e_var("x_resolution")),
                    s_assign("_u_y_resolution", e_var("y_resolution")),
                    s_assign("_u_filter", e_var("filter")),
                    s_assign("_u_blur", e_var("blur")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::resampleImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("resetImagePage")
                .param("page", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_page", e_var("page")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::resetImagePage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("rollImage")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::rollImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("rotationalBlurImage")
                .param("angle", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_angle", e_var("angle")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::rotationalBlurImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("roundCorners")
                .param("x_rounding", TypeExpr::Float)
                .param("y_rounding", TypeExpr::Float)
                .param_default("stroke_width", TypeExpr::Float, e_int(10))
                .param_default("displace", TypeExpr::Float, e_int(5))
                .param_default("size_correction", TypeExpr::Float, e_neg(e_int(6)))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x_rounding", e_var("x_rounding")),
                    s_assign("_u_y_rounding", e_var("y_rounding")),
                    s_assign("_u_stroke_width", e_var("stroke_width")),
                    s_assign("_u_displace", e_var("displace")),
                    s_assign("_u_size_correction", e_var("size_correction")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::roundCorners() is not supported in elephc")])),
                ]),
        )
        .method(
            method("sampleImage")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::sampleImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("segmentImage")
                .param("cOLORSPACE", TypeExpr::Int)
                .param("cluster_threshold", TypeExpr::Float)
                .param("smooth_threshold", TypeExpr::Float)
                .param_default("verbose", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_cOLORSPACE", e_var("cOLORSPACE")),
                    s_assign("_u_cluster_threshold", e_var("cluster_threshold")),
                    s_assign("_u_smooth_threshold", e_var("smooth_threshold")),
                    s_assign("_u_verbose", e_var("verbose")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::segmentImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("selectiveBlurImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .param("threshold", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_assign("_u_threshold", e_var("threshold")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::selectiveBlurImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("separateImageChannel")
                .param("channel", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::separateImageChannel() is not supported in elephc")])),
                ]),
        )
        .method(
            method("sepiaToneImage")
                .param("threshold", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_threshold", e_var("threshold")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::sepiaToneImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setBackgroundColor")
                .param_untyped("background")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_background", e_var("background")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setBackgroundColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setColorspace")
                .param("cOLORSPACE", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_cOLORSPACE", e_var("cOLORSPACE")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setColorspace() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setCompression")
                .param("compression", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_compression", e_var("compression")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setCompression() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFilename")
                .param("filename", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_filename", e_var("filename")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setFilename() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setFont")
                .param("font", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_font", e_var("font")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setFont() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setGravity")
                .param("gravity", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_gravity", e_var("gravity")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setGravity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImage")
                .param("replace", t_class("Imagick"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_replace", e_var("replace")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageAlphaChannel")
                .param("mode", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_mode", e_var("mode")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageAlphaChannel() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageArtifact")
                .param("artifact", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_artifact", e_var("artifact")),
                    s_assign("_u_value", e_var("value")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageArtifact() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageAttribute")
                .param("key", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_key", e_var("key")),
                    s_assign("_u_value", e_var("value")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageAttribute() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageBias")
                .param("bias", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_bias", e_var("bias")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageBias() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageBiasQuantum")
                .param("bias", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_u_bias", e_var("bias")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageBiasQuantum() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageBluePrimary")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageBluePrimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageBorderColor")
                .param_untyped("border")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_border", e_var("border")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageBorderColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageChannelDepth")
                .param("channel", TypeExpr::Int)
                .param("depth", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_assign("_u_depth", e_var("depth")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageChannelDepth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageClipMask")
                .param("clip_mask", t_class("Imagick"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_clip_mask", e_var("clip_mask")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageClipMask() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageColormapColor")
                .param("index", TypeExpr::Int)
                .param("color", t_class("ImagickPixel"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_index", e_var("index")),
                    s_assign("_u_color", e_var("color")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageColormapColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageColorspace")
                .param("colorspace", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_colorspace", e_var("colorspace")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageColorspace() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageCompose")
                .param("compose", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_compose", e_var("compose")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageCompose() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageCompression")
                .param("compression", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_compression", e_var("compression")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageCompression() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageDelay")
                .param("delay", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_delay", e_var("delay")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageDelay() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageDepth")
                .param("depth", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_depth", e_var("depth")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageDepth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageDispose")
                .param("dispose", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_dispose", e_var("dispose")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageDispose() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageExtent")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageExtent() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageFilename")
                .param("filename", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_filename", e_var("filename")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageFilename() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageGamma")
                .param("gamma", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_gamma", e_var("gamma")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageGamma() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageGravity")
                .param("gravity", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_gravity", e_var("gravity")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageGravity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageGreenPrimary")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageGreenPrimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageInterlaceScheme")
                .param("interlace_scheme", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_interlace_scheme", e_var("interlace_scheme")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageInterlaceScheme() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageInterpolateMethod")
                .param("method", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_method", e_var("method")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageInterpolateMethod() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageIterations")
                .param("iterations", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_iterations", e_var("iterations")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageIterations() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageMatte")
                .param("matte", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_matte", e_var("matte")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageMatte() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageMatteColor")
                .param_untyped("matte")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_matte", e_var("matte")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageMatteColor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageOpacity")
                .param("opacity", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_opacity", e_var("opacity")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageOpacity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageOrientation")
                .param("orientation", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_orientation", e_var("orientation")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageOrientation() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImagePage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImagePage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageProfile")
                .param("name", TypeExpr::Str)
                .param("profile", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_assign("_u_profile", e_var("profile")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageProfile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageProperty")
                .param("name", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_assign("_u_value", e_var("value")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageProperty() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageRedPrimary")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageRedPrimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageRenderingIntent")
                .param("rendering_intent", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_rendering_intent", e_var("rendering_intent")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageRenderingIntent() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageResolution")
                .param("x_resolution", TypeExpr::Float)
                .param("y_resolution", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x_resolution", e_var("x_resolution")),
                    s_assign("_u_y_resolution", e_var("y_resolution")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageResolution() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageScene")
                .param("scene", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_scene", e_var("scene")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageScene() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageTicksPerSecond")
                .param("ticks_per_second", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_ticks_per_second", e_var("ticks_per_second")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageTicksPerSecond() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageType")
                .param("image_type", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_image_type", e_var("image_type")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageType() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageUnits")
                .param("units", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_units", e_var("units")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageUnits() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageVirtualPixelMethod")
                .param("method", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_method", e_var("method")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageVirtualPixelMethod() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setImageWhitePoint")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setImageWhitePoint() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setInterlaceScheme")
                .param("interlace_scheme", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_interlace_scheme", e_var("interlace_scheme")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setInterlaceScheme() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setOption")
                .param("key", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_key", e_var("key")),
                    s_assign("_u_value", e_var("value")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setOption() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setPage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setPage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setPointSize")
                .param("point_size", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_point_size", e_var("point_size")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setPointSize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setProgressMonitor")
                .param_untyped("callback")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_callback", e_var("callback")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setProgressMonitor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setRegistry")
                .static_()
                .param("key", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_key", e_var("key")),
                    s_assign("_u_value", e_var("value")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setRegistry() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setResolution")
                .param("x_resolution", TypeExpr::Float)
                .param("y_resolution", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_x_resolution", e_var("x_resolution")),
                    s_assign("_u_y_resolution", e_var("y_resolution")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setResolution() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setResourceLimit")
                .static_()
                .param("type", TypeExpr::Int)
                .param("limit", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_type", e_var("type")),
                    s_assign("_u_limit", e_var("limit")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setResourceLimit() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setSamplingFactors")
                .param("factors", t_array())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_factors", e_var("factors")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setSamplingFactors() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setSize")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setSize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setSizeOffset")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .param("offset", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_assign("_u_offset", e_var("offset")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setSizeOffset() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setType")
                .param("image_type", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_image_type", e_var("image_type")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::setType() is not supported in elephc")])),
                ]),
        )
        .method(
            method("shadeImage")
                .param("gray", TypeExpr::Bool)
                .param("azimuth", TypeExpr::Float)
                .param("elevation", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_gray", e_var("gray")),
                    s_assign("_u_azimuth", e_var("azimuth")),
                    s_assign("_u_elevation", e_var("elevation")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::shadeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("shadowImage")
                .param("opacity", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_opacity", e_var("opacity")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::shadowImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("shaveImage")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::shaveImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("shearImage")
                .param_untyped("background")
                .param("x_shear", TypeExpr::Float)
                .param("y_shear", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_background", e_var("background")),
                    s_assign("_u_x_shear", e_var("x_shear")),
                    s_assign("_u_y_shear", e_var("y_shear")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::shearImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("sigmoidalContrastImage")
                .param("sharpen", TypeExpr::Bool)
                .param("alpha", TypeExpr::Float)
                .param("beta", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_sharpen", e_var("sharpen")),
                    s_assign("_u_alpha", e_var("alpha")),
                    s_assign("_u_beta", e_var("beta")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::sigmoidalContrastImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("sketchImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .param("angle", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_assign("_u_angle", e_var("angle")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::sketchImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("smushImages")
                .param("stack", TypeExpr::Bool)
                .param("offset", TypeExpr::Int)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_stack", e_var("stack")),
                    s_assign("_u_offset", e_var("offset")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::smushImages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("solarizeImage")
                .param("threshold", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_threshold", e_var("threshold")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::solarizeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("sparseColorImage")
                .param("sPARSE_METHOD", TypeExpr::Int)
                .param("arguments", t_array())
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_sPARSE_METHOD", e_var("sPARSE_METHOD")),
                    s_assign("_u_arguments", e_var("arguments")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::sparseColorImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("spliceImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::spliceImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("spreadImage")
                .param("radius", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::spreadImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("statisticImage")
                .param("type", TypeExpr::Int)
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_type", e_var("type")),
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::statisticImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("steganoImage")
                .param("watermark_wand", t_class("Imagick"))
                .param("offset", TypeExpr::Int)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_watermark_wand", e_var("watermark_wand")),
                    s_assign("_u_offset", e_var("offset")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::steganoImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("stereoImage")
                .param("offset_wand", t_class("Imagick"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_offset_wand", e_var("offset_wand")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::stereoImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("stripImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::stripImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("subImageMatch")
                .param("imagick", t_class("Imagick"))
                .param_by_ref("offset", Some(t_array()))
                .param_by_ref("similarity", Some(TypeExpr::Float))
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_imagick", e_var("imagick")),
                    s_assign("_u_offset", e_var("offset")),
                    s_assign("_u_similarity", e_var("similarity")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::subImageMatch() is not supported in elephc")])),
                ]),
        )
        .method(
            method("thresholdImage")
                .param("threshold", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_threshold", e_var("threshold")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::thresholdImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("tintImage")
                .param_untyped("tint")
                .param_untyped("opacity")
                .param_default("legacy", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_tint", e_var("tint")),
                    s_assign("_u_opacity", e_var("opacity")),
                    s_assign("_u_legacy", e_var("legacy")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::tintImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("__toString")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::__toString() is not supported in elephc")])),
                ]),
        )
        .method(
            method("transformImage")
                .param("crop", TypeExpr::Str)
                .param("geometry", TypeExpr::Str)
                .returns(t_class("Imagick"))
                .body(vec![
                    s_assign("_u_crop", e_var("crop")),
                    s_assign("_u_geometry", e_var("geometry")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::transformImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("transformImageColorspace")
                .param("colorspace", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_colorspace", e_var("colorspace")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::transformImageColorspace() is not supported in elephc")])),
                ]),
        )
        .method(
            method("transparentPaintImage")
                .param_untyped("target")
                .param("alpha", TypeExpr::Float)
                .param("fuzz", TypeExpr::Float)
                .param("invert", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_target", e_var("target")),
                    s_assign("_u_alpha", e_var("alpha")),
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_assign("_u_invert", e_var("invert")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::transparentPaintImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("transposeImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::transposeImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("transverseImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::transverseImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("trimImage")
                .param("fuzz", TypeExpr::Float)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::trimImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("uniqueImageColors")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::uniqueImageColors() is not supported in elephc")])),
                ]),
        )
        .method(
            method("unsharpMaskImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .param("amount", TypeExpr::Float)
                .param("threshold", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_assign("_u_amount", e_var("amount")),
                    s_assign("_u_threshold", e_var("threshold")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::unsharpMaskImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("vignetteImage")
                .param("blackPoint", TypeExpr::Float)
                .param("whitePoint", TypeExpr::Float)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_blackPoint", e_var("blackPoint")),
                    s_assign("_u_whitePoint", e_var("whitePoint")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::vignetteImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("whiteThresholdImage")
                .param_untyped("threshold")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_threshold", e_var("threshold")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::whiteThresholdImage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("writeImageFile")
                .param_untyped("filehandle")
                .param_default("format", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_filehandle", e_var("filehandle")),
                    s_assign("_u_format", e_var("format")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::writeImageFile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("writeImagesFile")
                .param_untyped("filehandle")
                .param_default("format", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_filehandle", e_var("filehandle")),
                    s_assign("_u_format", e_var("format")),
                    s_throw(e_new("ImagickException", vec![e_str("Imagick::writeImagesFile() is not supported in elephc")])),
                ]),
        )
        // --- end auto-generated API-surface stubs ---
        .build()
}

/// `ImagickPixelIterator` — transcribed from the PHP form.
fn decl_class_imagickpixeliterator() -> Stmt {
    class("ImagickPixelIterator")
        .implements("Iterator")
        .private_prop("wand", TypeExpr::Int, Some(e_int(0)))
        .private_prop("row", TypeExpr::Int, Some(e_int(0)))
        .private_prop("width", TypeExpr::Int, Some(e_int(0)))
        .private_prop("height", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param("wand", t_class("Imagick"))
                .body(vec![
                    s_prop_assign(e_this(), "wand", e_method_call(e_var("wand"), "_wandHandle", vec![])),
                    s_assign("_w", e_call("elephc_imagick_cur_width", vec![e_this_prop("wand")])),
                    s_assign("_h", e_call("elephc_imagick_cur_height", vec![e_this_prop("wand")])),
                    s_prop_assign(e_this(), "width", e_ternary(e_binop(e_var("_w"), BinOp::Lt, e_int(0)), e_int(0), e_var("_w"))),
                    s_prop_assign(e_this(), "height", e_ternary(e_binop(e_var("_h"), BinOp::Lt, e_int(0)), e_int(0), e_var("_h"))),
                    s_prop_assign(e_this(), "row", e_int(0)),
                ]),
        )
        .method(
            method("getCurrentIteratorRow")
                .returns(t_array())
                .body(vec![
                    s_assign("_pixels", e_array(vec![])),
                    s_for(Some(s_assign("_x", e_int(0))), Some(e_binop(e_var("_x"), BinOp::Lt, e_this_prop("width"))), Some(s_expr(e_post_inc("_x"))), vec![
                        s_assign("_packed", e_call("elephc_imagick_pixel_color", vec![e_this_prop("wand"), e_var("_x"), e_this_prop("row")])),
                        s_array_push("_pixels", e_call("_imagick_pixel_from_int", vec![e_var("_packed")])),
                    ]),
                    s_return(e_var("_pixels")),
                ]),
        )
        .method(
            method("getNextIteratorRow")
                .returns(t_array())
                .body(vec![
                    s_prop_assign(e_this(), "row", e_binop(e_this_prop("row"), BinOp::Add, e_int(1))),
                    s_return(e_method_call(e_this(), "getCurrentIteratorRow", vec![])),
                ]),
        )
        .method(
            method("getIteratorIndex")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("row")),
                ]),
        )
        .method(
            method("setIteratorRow")
                .param("row", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_var("row"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("row"), BinOp::GtEq, e_this_prop("height"))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "row", e_var("row")),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("rewind")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "row", e_int(0)),
                ]),
        )
        .method(
            method("valid")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_this_prop("row"), BinOp::Lt, e_this_prop("height"))),
                ]),
        )
        .method(
            method("current")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_method_call(e_this(), "getCurrentIteratorRow", vec![])),
                ]),
        )
        .method(
            method("key")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_this_prop("row")),
                ]),
        )
        .method(
            method("next")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "row", e_binop(e_this_prop("row"), BinOp::Add, e_int(1))),
                ]),
        )
        .method(
            method("clear")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("destroy")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(true)),
                ]),
        )
        // --- begin auto-generated API-surface throwing stubs (do not edit; regen via crates/elephc-image/tools/gen_image_api_stubs.py) ---
        .method(
            method("getIteratorRow")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("ImagickPixelIteratorException", vec![e_str("ImagickPixelIterator::getIteratorRow() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getPreviousIteratorRow")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("ImagickPixelIteratorException", vec![e_str("ImagickPixelIterator::getPreviousIteratorRow() is not supported in elephc")])),
                ]),
        )
        .method(
            method("newPixelIterator")
                .param("wand", t_class("Imagick"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_wand", e_var("wand")),
                    s_throw(e_new("ImagickPixelIteratorException", vec![e_str("ImagickPixelIterator::newPixelIterator() is not supported in elephc")])),
                ]),
        )
        .method(
            method("newPixelRegionIterator")
                .param("wand", t_class("Imagick"))
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_u_wand", e_var("wand")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_throw(e_new("ImagickPixelIteratorException", vec![e_str("ImagickPixelIterator::newPixelRegionIterator() is not supported in elephc")])),
                ]),
        )
        .method(
            method("resetIterator")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickPixelIteratorException", vec![e_str("ImagickPixelIterator::resetIterator() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setIteratorFirstRow")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickPixelIteratorException", vec![e_str("ImagickPixelIterator::setIteratorFirstRow() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setIteratorLastRow")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickPixelIteratorException", vec![e_str("ImagickPixelIterator::setIteratorLastRow() is not supported in elephc")])),
                ]),
        )
        .method(
            method("syncIterator")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_throw(e_new("ImagickPixelIteratorException", vec![e_str("ImagickPixelIterator::syncIterator() is not supported in elephc")])),
                ]),
        )
        // --- end auto-generated API-surface stubs ---
        .build()
}

/// `GmagickException` — transcribed from the PHP form.
fn decl_class_gmagickexception() -> Stmt {
    class("GmagickException")
        .extends("Exception")
        .build()
}

/// `GmagickDrawException` — transcribed from the PHP form.
fn decl_class_gmagickdrawexception() -> Stmt {
    class("GmagickDrawException")
        .extends("Exception")
        .build()
}

/// `GmagickPixelException` — transcribed from the PHP form.
fn decl_class_gmagickpixelexception() -> Stmt {
    class("GmagickPixelException")
        .extends("Exception")
        .build()
}

/// `_gmagick_parse_color` — transcribed from the PHP form.
fn decl_fn_gmagick_parse_color() -> Stmt {
    function("_gmagick_parse_color")
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_try(vec![
                s_return(e_call("_imagick_parse_color", vec![e_var("c")])),
            ], vec![
                (vec!["ImagickPixelException"], Some("e"), vec![
                    s_throw(e_new("GmagickPixelException", vec![e_method_call(e_var("e"), "getMessage", vec![])])),
                ]),
            ], None),
        ])
        .build()
}

/// `_gmagick_norm_color` — transcribed from the PHP form.
fn decl_fn_gmagick_norm_color() -> Stmt {
    function("_gmagick_norm_color")
        .param_untyped("color")
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_call("is_string", vec![e_var("color")]),
                vec![
                    s_return(e_call("_gmagick_parse_color", vec![e_var("color")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("color"), "GmagickPixel"),
                vec![
                    s_return(e_cast(CastType::Int, e_prop(e_var("color"), "packed"))),
                ],
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
        .build()
}

/// `_gmagick_pixel_from_int` — transcribed from the PHP form.
fn decl_fn_gmagick_pixel_from_int() -> Stmt {
    function("_gmagick_pixel_from_int")
        .param("packed", TypeExpr::Int)
        .returns(t_class("GmagickPixel"))
        .body(vec![
            s_assign("_p", e_new("GmagickPixel", vec![e_str("black")])),
            s_prop_assign(e_var("_p"), "packed", e_var("packed")),
            s_return(e_var("_p")),
        ])
        .build()
}

/// `GmagickPixel` — transcribed from the PHP form.
fn decl_class_gmagickpixel() -> Stmt {
    class("GmagickPixel")
        .prop("packed", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param_default("color", TypeExpr::Str, e_str("black"))
                .body(vec![
                    s_prop_assign(e_this(), "packed", e_call("_gmagick_parse_color", vec![e_var("color")])),
                ]),
        )
        .method(
            method("setColor")
                .param("color", TypeExpr::Str)
                .returns(t_class("GmagickPixel"))
                .body(vec![
                    s_prop_assign(e_this(), "packed", e_call("_gmagick_parse_color", vec![e_var("color")])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("getColor")
                .param_default("normalized", TypeExpr::Int, e_int(0))
                .body(vec![
                    s_assign("_r", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))),
                    s_assign("_g", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))),
                    s_assign("_b", e_binop(e_this_prop("packed"), BinOp::BitAnd, e_int(255))),
                    s_assign("_gd", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(24)), BinOp::BitAnd, e_int(127))),
                    s_assign("_a", e_binop(e_int(255), BinOp::Sub, e_cast(CastType::Int, e_binop(e_binop(e_var("_gd"), BinOp::Mul, e_int(255)), BinOp::Div, e_int(127))))),
                    s_if(
                        e_binop(e_var("normalized"), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("r"), e_binop(e_var("_r"), BinOp::Div, e_int(255))), (e_str("g"), e_binop(e_var("_g"), BinOp::Div, e_int(255))), (e_str("b"), e_binop(e_var("_b"), BinOp::Div, e_int(255))), (e_str("a"), e_binop(e_var("_a"), BinOp::Div, e_int(255)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array_assoc(vec![(e_str("r"), e_var("_r")), (e_str("g"), e_var("_g")), (e_str("b"), e_var("_b")), (e_str("a"), e_var("_a"))])),
                ]),
        )
        .method(
            method("getColorAsString")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_r", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))),
                    s_assign("_g", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))),
                    s_assign("_b", e_binop(e_this_prop("packed"), BinOp::BitAnd, e_int(255))),
                    s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("srgb("), BinOp::Concat, e_var("_r")), BinOp::Concat, e_str(",")), BinOp::Concat, e_var("_g")), BinOp::Concat, e_str(",")), BinOp::Concat, e_var("_b")), BinOp::Concat, e_str(")"))),
                ]),
        )
        .method(
            method("getColorValue")
                .param("color", TypeExpr::Int)
                .returns(TypeExpr::Float)
                .body(vec![
                    s_assign("_r", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))),
                    s_assign("_g", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))),
                    s_assign("_b", e_binop(e_this_prop("packed"), BinOp::BitAnd, e_int(255))),
                    s_assign("_gd", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(24)), BinOp::BitAnd, e_int(127))),
                    s_assign("_a", e_binop(e_int(255), BinOp::Sub, e_cast(CastType::Int, e_binop(e_binop(e_var("_gd"), BinOp::Mul, e_int(255)), BinOp::Div, e_int(127))))),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_return(e_binop(e_var("_r"), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_return(e_binop(e_var("_g"), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_return(e_binop(e_var("_b"), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(8)),
                        vec![
                            s_return(e_binop(e_var("_a"), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(7)),
                        vec![
                            s_return(e_binop(e_binop(e_int(255), BinOp::Sub, e_var("_a")), BinOp::Div, e_int(255))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_float(0.0)),
                ]),
        )
        .method(
            method("setColorValue")
                .param("color", TypeExpr::Int)
                .param("value", TypeExpr::Float)
                .returns(t_class("GmagickPixel"))
                .body(vec![
                    s_assign("_r", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(16)), BinOp::BitAnd, e_int(255))),
                    s_assign("_g", e_binop(e_binop(e_this_prop("packed"), BinOp::ShiftRight, e_int(8)), BinOp::BitAnd, e_int(255))),
                    s_assign("_b", e_binop(e_this_prop("packed"), BinOp::BitAnd, e_int(255))),
                    s_assign("_v", e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("value"), BinOp::Mul, e_int(255))]))),
                    s_if(
                        e_binop(e_var("_v"), BinOp::Lt, e_int(0)),
                        vec![
                            s_assign("_v", e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_v"), BinOp::Gt, e_int(255)),
                        vec![
                            s_assign("_v", e_int(255)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_assign("_r", e_var("_v")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_assign("_g", e_var("_v")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("color"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_assign("_b", e_var("_v")),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "packed", e_binop(e_binop(e_binop(e_var("_r"), BinOp::ShiftLeft, e_int(16)), BinOp::BitOr, e_binop(e_var("_g"), BinOp::ShiftLeft, e_int(8))), BinOp::BitOr, e_var("_b"))),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("clear")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("destroy")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(true)),
                ]),
        )
        // --- begin auto-generated API-surface throwing stubs (do not edit; regen via crates/elephc-image/tools/gen_image_api_stubs.py) ---
        .method(
            method("getcolorcount")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickPixelException", vec![e_str("GmagickPixel::getcolorcount() is not supported in elephc")])),
                ]),
        )
        // --- end auto-generated API-surface stubs ---
        .build()
}

/// `GmagickDraw` — transcribed from the PHP form.
fn decl_class_gmagickdraw() -> Stmt {
    class("GmagickDraw")
        .private_prop("draw", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .body(vec![
                    s_prop_assign(e_this(), "draw", e_call("elephc_idraw_new", vec![])),
                ]),
        )
        .method(
            method("_gmagickHandle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("draw")),
                ]),
        )
        .method(
            method("setFillColor")
                .param_untyped("color")
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_expr(e_call("elephc_idraw_set_fill", vec![e_this_prop("draw"), e_call("_gmagick_norm_color", vec![e_var("color")])])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("setStrokeColor")
                .param_untyped("color")
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_expr(e_call("elephc_idraw_set_stroke", vec![e_this_prop("draw"), e_call("_gmagick_norm_color", vec![e_var("color")])])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("setStrokeWidth")
                .param("width", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_expr(e_call("elephc_idraw_set_stroke_width", vec![e_this_prop("draw"), e_cast(CastType::Int, e_call("round", vec![e_var("width")]))])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("line")
                .param("sx", TypeExpr::Float)
                .param("sy", TypeExpr::Float)
                .param("ex", TypeExpr::Float)
                .param("ey", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_expr(e_call("elephc_idraw_line", vec![e_this_prop("draw"), e_cast(CastType::Int, e_call("round", vec![e_var("sx")])), e_cast(CastType::Int, e_call("round", vec![e_var("sy")])), e_cast(CastType::Int, e_call("round", vec![e_var("ex")])), e_cast(CastType::Int, e_call("round", vec![e_var("ey")]))])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("rectangle")
                .param("x1", TypeExpr::Float)
                .param("y1", TypeExpr::Float)
                .param("x2", TypeExpr::Float)
                .param("y2", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_expr(e_call("elephc_idraw_rectangle", vec![e_this_prop("draw"), e_cast(CastType::Int, e_call("round", vec![e_var("x1")])), e_cast(CastType::Int, e_call("round", vec![e_var("y1")])), e_cast(CastType::Int, e_call("round", vec![e_var("x2")])), e_cast(CastType::Int, e_call("round", vec![e_var("y2")]))])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("ellipse")
                .param("ox", TypeExpr::Float)
                .param("oy", TypeExpr::Float)
                .param("rx", TypeExpr::Float)
                .param("ry", TypeExpr::Float)
                .param("start", TypeExpr::Float)
                .param("end", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_oxy", e_call("_imagick_pack2", vec![e_cast(CastType::Int, e_call("round", vec![e_var("ox")])), e_cast(CastType::Int, e_call("round", vec![e_var("oy")]))])),
                    s_assign("_rxy", e_call("_imagick_pack2", vec![e_cast(CastType::Int, e_call("round", vec![e_var("rx")])), e_cast(CastType::Int, e_call("round", vec![e_var("ry")]))])),
                    s_assign("_se", e_call("_imagick_pack2", vec![e_cast(CastType::Int, e_call("round", vec![e_var("start")])), e_cast(CastType::Int, e_call("round", vec![e_var("end")]))])),
                    s_expr(e_call("elephc_idraw_ellipse", vec![e_this_prop("draw"), e_var("_oxy"), e_var("_rxy"), e_var("_se")])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("point")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_expr(e_call("elephc_idraw_point", vec![e_this_prop("draw"), e_cast(CastType::Int, e_call("round", vec![e_var("x")])), e_cast(CastType::Int, e_call("round", vec![e_var("y")]))])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("polygon")
                .param("coordinates", t_array())
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_expr(e_call("elephc_idraw_poly_reset", vec![e_this_prop("draw")])),
                    s_assign("_n", e_call("count", vec![e_var("coordinates")])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_var("_n"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_px", e_cast(CastType::Int, e_call("round", vec![e_index(e_index(e_var("coordinates"), e_var("_i")), e_str("x"))]))),
                        s_assign("_py", e_cast(CastType::Int, e_call("round", vec![e_index(e_index(e_var("coordinates"), e_var("_i")), e_str("y"))]))),
                        s_expr(e_call("elephc_idraw_poly_point", vec![e_this_prop("draw"), e_var("_px"), e_var("_py")])),
                    ]),
                    s_expr(e_call("elephc_idraw_polygon", vec![e_this_prop("draw")])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("annotate")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .param("text", TypeExpr::Str)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_t", e_var("text")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::annotate() requires FreeType text, which is not supported in elephc")])),
                ]),
        )
        .method(
            method("clear")
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_expr(e_call("elephc_idraw_clear", vec![e_this_prop("draw")])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("destroy")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_idraw_destroy", vec![e_this_prop("draw")])),
                    s_prop_assign(e_this(), "draw", e_int(0)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_expr(e_call("elephc_idraw_destroy", vec![e_this_prop("draw")])),
                ]),
        )
        // --- begin auto-generated API-surface throwing stubs (do not edit; regen via crates/elephc-image/tools/gen_image_api_stubs.py) ---
        .method(
            method("arc")
                .param("sx", TypeExpr::Float)
                .param("sy", TypeExpr::Float)
                .param("ex", TypeExpr::Float)
                .param("ey", TypeExpr::Float)
                .param("sd", TypeExpr::Float)
                .param("ed", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_sx", e_var("sx")),
                    s_assign("_u_sy", e_var("sy")),
                    s_assign("_u_ex", e_var("ex")),
                    s_assign("_u_ey", e_var("ey")),
                    s_assign("_u_sd", e_var("sd")),
                    s_assign("_u_ed", e_var("ed")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::arc() is not supported in elephc")])),
                ]),
        )
        .method(
            method("bezier")
                .param("coordinate_array", t_array())
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_coordinate_array", e_var("coordinate_array")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::bezier() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getfillcolor")
                .returns(t_class("GmagickPixel"))
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::getfillcolor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getfillopacity")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::getfillopacity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getfont")
                .returns(t_mixed())
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::getfont() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getfontsize")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::getfontsize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getfontstyle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::getfontstyle() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getfontweight")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::getfontweight() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getstrokecolor")
                .returns(t_class("GmagickPixel"))
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::getstrokecolor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getstrokeopacity")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::getstrokeopacity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getstrokewidth")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::getstrokewidth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("gettextdecoration")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::gettextdecoration() is not supported in elephc")])),
                ]),
        )
        .method(
            method("gettextencoding")
                .returns(t_mixed())
                .body(vec![
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::gettextencoding() is not supported in elephc")])),
                ]),
        )
        .method(
            method("polyline")
                .param("coordinate_array", t_array())
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_coordinate_array", e_var("coordinate_array")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::polyline() is not supported in elephc")])),
                ]),
        )
        .method(
            method("rotate")
                .param("degrees", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_degrees", e_var("degrees")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::rotate() is not supported in elephc")])),
                ]),
        )
        .method(
            method("roundrectangle")
                .param("x1", TypeExpr::Float)
                .param("y1", TypeExpr::Float)
                .param("x2", TypeExpr::Float)
                .param("y2", TypeExpr::Float)
                .param("rx", TypeExpr::Float)
                .param("ry", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_x1", e_var("x1")),
                    s_assign("_u_y1", e_var("y1")),
                    s_assign("_u_x2", e_var("x2")),
                    s_assign("_u_y2", e_var("y2")),
                    s_assign("_u_rx", e_var("rx")),
                    s_assign("_u_ry", e_var("ry")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::roundrectangle() is not supported in elephc")])),
                ]),
        )
        .method(
            method("scale")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::scale() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setfillopacity")
                .param("fill_opacity", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_fill_opacity", e_var("fill_opacity")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::setfillopacity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setfont")
                .param("font", TypeExpr::Str)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_font", e_var("font")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::setfont() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setfontsize")
                .param("pointsize", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_pointsize", e_var("pointsize")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::setfontsize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setfontstyle")
                .param("style", TypeExpr::Int)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_style", e_var("style")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::setfontstyle() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setfontweight")
                .param("weight", TypeExpr::Int)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_weight", e_var("weight")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::setfontweight() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setstrokeopacity")
                .param("stroke_opacity", TypeExpr::Float)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_stroke_opacity", e_var("stroke_opacity")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::setstrokeopacity() is not supported in elephc")])),
                ]),
        )
        .method(
            method("settextdecoration")
                .param("decoration", TypeExpr::Int)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_decoration", e_var("decoration")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::settextdecoration() is not supported in elephc")])),
                ]),
        )
        .method(
            method("settextencoding")
                .param("encoding", TypeExpr::Str)
                .returns(t_class("GmagickDraw"))
                .body(vec![
                    s_assign("_u_encoding", e_var("encoding")),
                    s_throw(e_new("GmagickDrawException", vec![e_str("GmagickDraw::settextencoding() is not supported in elephc")])),
                ]),
        )
        // --- end auto-generated API-surface stubs ---
        .build()
}

/// `Gmagick` — transcribed from the PHP form.
fn decl_class_gmagick() -> Stmt {
    class("Gmagick")
        .constant("FILTER_UNDEFINED", e_int(0))
        .constant("FILTER_POINT", e_int(1))
        .constant("FILTER_BOX", e_int(2))
        .constant("FILTER_TRIANGLE", e_int(3))
        .constant("FILTER_HERMITE", e_int(4))
        .constant("FILTER_HANNING", e_int(5))
        .constant("FILTER_HAMMING", e_int(6))
        .constant("FILTER_BLACKMAN", e_int(7))
        .constant("FILTER_GAUSSIAN", e_int(8))
        .constant("FILTER_QUADRATIC", e_int(9))
        .constant("FILTER_CUBIC", e_int(10))
        .constant("FILTER_CATROM", e_int(11))
        .constant("FILTER_MITCHELL", e_int(12))
        .constant("FILTER_LANCZOS", e_int(22))
        .constant("FILTER_SINC", e_int(19))
        .constant("COMPOSITE_DEFAULT", e_int(40))
        .constant("COMPOSITE_OVER", e_int(40))
        .constant("COMPOSITE_COPY", e_int(42))
        .constant("COMPOSITE_MULTIPLY", e_int(30))
        .constant("CHANNEL_RED", e_int(1))
        .constant("CHANNEL_GREEN", e_int(2))
        .constant("CHANNEL_BLUE", e_int(4))
        .constant("CHANNEL_ALPHA", e_int(8))
        .constant("CHANNEL_OPACITY", e_int(8))
        .constant("CHANNEL_ALL", e_int(134217727))
        .constant("COLOR_BLACK", e_int(0))
        .constant("COLOR_BLUE", e_int(1))
        .constant("COLOR_GREEN", e_int(3))
        .constant("COLOR_RED", e_int(4))
        .constant("COLOR_OPACITY", e_int(7))
        .constant("COLOR_ALPHA", e_int(8))
        .constant("IMGTYPE_UNDEFINED", e_int(0))
        .constant("IMGTYPE_GRAYSCALE", e_int(2))
        .constant("IMGTYPE_PALETTE", e_int(3))
        .constant("IMGTYPE_TRUECOLOR", e_int(6))
        .private_prop("wand", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param_default("filename", t_nullable(TypeExpr::Str), e_null())
                .body(vec![
                    s_prop_assign(e_this(), "wand", e_call("elephc_imagick_new", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("filename"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("filename"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_expr(e_method_call(e_this(), "readImage", vec![e_cast(CastType::String, e_var("filename"))])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("_wandHandle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("wand")),
                ]),
        )
        .method(
            method("readImage")
                .param("filename", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_read_file", vec![e_this_prop("wand"), e_var("filename")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_binop(e_binop(e_str("Gmagick::readimage(): unable to read '"), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("'"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("readImageBlob")
                .param("image", TypeExpr::Str)
                .param_default("filename", TypeExpr::Str, e_str(""))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_name", e_var("filename")),
                    s_assign("_len", e_call("strlen", vec![e_var("image")])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::LtEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::readimageblob(): empty blob")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_buf", e_call("elephc_img_stage_ptr", vec![e_var("_len")])),
                    s_if(
                        e_call("__elephc_ptr_is_null", vec![e_var("_buf")]),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::readimageblob(): allocation failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_ptr_write_string", vec![e_var("_buf"), e_var("image")])),
                    s_if(
                        e_binop(e_call("elephc_imagick_read_blob", vec![e_this_prop("wand"), e_var("_len")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::readimageblob(): unrecognized image data")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("newImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param_untyped("background")
                .param_default("format", TypeExpr::Str, e_str(""))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_bg", e_call("_gmagick_norm_color", vec![e_var("background")])),
                    s_assign("_fmt", e_ternary(e_binop(e_var("format"), BinOp::StrictEq, e_str("")), e_int(0), e_call("_imagick_fmt_to_code", vec![e_var("format")]))),
                    s_if(
                        e_binop(e_call("elephc_imagick_new_image", vec![e_this_prop("wand"), e_var("width"), e_var("height"), e_var("_bg"), e_var("_fmt")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::newimage(): invalid dimensions")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("addImage")
                .param("source", t_class("Gmagick"))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_add_image", vec![e_this_prop("wand"), e_method_call(e_var("source"), "_wandHandle", vec![])]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::addimage(): no source image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("writeImage")
                .param("filename", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_fmt", e_call("_imagick_fmt_from_path", vec![e_var("filename")])),
                    s_if(
                        e_binop(e_call("elephc_imagick_write_file", vec![e_this_prop("wand"), e_var("filename"), e_var("_fmt")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_binop(e_binop(e_str("Gmagick::writeimage(): unable to write '"), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("'"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("getImageBlob")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_len", e_call("elephc_imagick_get_blob", vec![e_this_prop("wand"), e_int(0)])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimageblob(): no image or encode failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_bytes", e_call("__elephc_ptr_read_string", vec![e_call("elephc_img_encoded_ptr", vec![]), e_var("_len")])),
                    s_expr(e_call("elephc_img_encoded_clear", vec![])),
                    s_return(e_var("_bytes")),
                ]),
        )
        .method(
            method("setImageFormat")
                .param("format", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_code", e_call("_imagick_fmt_to_code", vec![e_var("format")])),
                    s_if(
                        e_binop(e_var("_code"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_binop(e_binop(e_str("Gmagick::setimageformat(): unsupported format '"), BinOp::Concat, e_var("format")), BinOp::Concat, e_str("'"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_imagick_set_format", vec![e_this_prop("wand"), e_var("_code")])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("getImageFormat")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("_imagick_code_to_fmt", vec![e_call("elephc_imagick_get_format", vec![e_this_prop("wand")])])),
                ]),
        )
        .method(
            method("setCompressionQuality")
                .param("quality", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_expr(e_call("elephc_imagick_set_quality", vec![e_this_prop("wand"), e_var("quality")])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("getCompressionQuality")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("_q", e_call("elephc_imagick_get_quality", vec![e_this_prop("wand")])),
                    s_return(e_ternary(e_binop(e_var("_q"), BinOp::Lt, e_int(0)), e_int(0), e_var("_q"))),
                ]),
        )
        .method(
            method("getImageWidth")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_cur_width", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("getImageHeight")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_cur_height", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("getImageGeometry")
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("width"), e_method_call(e_this(), "getImageWidth", vec![])), (e_str("height"), e_method_call(e_this(), "getImageHeight", vec![]))])),
                ]),
        )
        .method(
            method("resizeImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("filter", TypeExpr::Int)
                .param("factor", TypeExpr::Float)
                .param_default("fit", TypeExpr::Bool, e_bool(false))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_filter", e_var("filter")),
                    s_assign("_u_factor", e_var("factor")),
                    s_assign("_u_fit", e_var("fit")),
                    s_if(
                        e_binop(e_call("elephc_imagick_resize", vec![e_this_prop("wand"), e_var("width"), e_var("height")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::resizeimage(): resize failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("scaleImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param_default("fit", TypeExpr::Bool, e_bool(false))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_fit", e_var("fit")),
                    s_if(
                        e_binop(e_call("elephc_imagick_scale", vec![e_this_prop("wand"), e_var("width"), e_var("height")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::scaleimage(): scale failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("thumbnailImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param_default("fit", TypeExpr::Bool, e_bool(false))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_fit", e_var("fit")),
                    s_assign("_ow", e_method_call(e_this(), "getImageWidth", vec![])),
                    s_assign("_oh", e_method_call(e_this(), "getImageHeight", vec![])),
                    s_assign("_w", e_var("width")),
                    s_assign("_h", e_var("height")),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("width"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_var("height"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_var("_oh"), BinOp::Gt, e_int(0))),
                        vec![
                            s_assign("_w", e_cast(CastType::Int, e_call("round", vec![e_binop(e_binop(e_var("_ow"), BinOp::Mul, e_var("height")), BinOp::Div, e_var("_oh"))]))),
                        ],
                        vec![
                        (e_binop(e_binop(e_binop(e_var("height"), BinOp::Eq, e_int(0)), BinOp::And, e_binop(e_var("width"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_var("_ow"), BinOp::Gt, e_int(0))), vec![
                            s_assign("_h", e_cast(CastType::Int, e_call("round", vec![e_binop(e_binop(e_var("_oh"), BinOp::Mul, e_var("width")), BinOp::Div, e_var("_ow"))]))),
                        ]),
                    ],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_w"), BinOp::Lt, e_int(1)),
                        vec![
                            s_assign("_w", e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_h"), BinOp::Lt, e_int(1)),
                        vec![
                            s_assign("_h", e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_imagick_resize", vec![e_this_prop("wand"), e_var("_w"), e_var("_h")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::thumbnailimage(): failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("cropImage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_crop", vec![e_this_prop("wand"), e_var("width"), e_var("height"), e_var("x"), e_var("y")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::cropimage(): invalid crop region")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("rotateImage")
                .param_untyped("color")
                .param("degrees", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_bg", e_call("_gmagick_norm_color", vec![e_var("color")])),
                    s_assign("_mdeg", e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("degrees"), BinOp::Mul, e_int(1000))]))),
                    s_if(
                        e_binop(e_call("elephc_imagick_rotate", vec![e_this_prop("wand"), e_var("_mdeg"), e_var("_bg")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::rotateimage(): rotate failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("flipImage")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_flip", vec![e_this_prop("wand")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::flipimage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("flopImage")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_flop", vec![e_this_prop("wand")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::flopimage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("blurImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_sig", e_ternary(e_binop(e_var("sigma"), BinOp::Gt, e_float(0.0)), e_var("sigma"), e_var("radius"))),
                    s_if(
                        e_binop(e_call("elephc_imagick_blur", vec![e_this_prop("wand"), e_cast(CastType::Int, e_call("round", vec![e_binop(e_var("_sig"), BinOp::Mul, e_int(1000))]))]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::blurimage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("gaussianBlurImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_return(e_method_call(e_this(), "blurImage", vec![e_var("radius"), e_var("sigma")])),
                ]),
        )
        .method(
            method("modulateImage")
                .param("brightness", TypeExpr::Float)
                .param("saturation", TypeExpr::Float)
                .param("hue", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_modulate", vec![e_this_prop("wand"), e_cast(CastType::Int, e_call("round", vec![e_var("brightness")])), e_cast(CastType::Int, e_call("round", vec![e_var("saturation")])), e_cast(CastType::Int, e_call("round", vec![e_var("hue")]))]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::modulateimage(): no image")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("compositeImage")
                .param("source", t_class("Gmagick"))
                .param("compose", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_rc", e_call("elephc_imagick_composite", vec![e_this_prop("wand"), e_method_call(e_var("source"), "_wandHandle", vec![]), e_var("compose"), e_var("x"), e_var("y")])),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::StrictEq, e_neg(e_int(2))),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_binop(e_binop(e_str("Gmagick::compositeimage(): composite operator "), BinOp::Concat, e_var("compose")), BinOp::Concat, e_str(" is not supported in elephc"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_rc"), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::compositeimage(): composite failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("drawImage")
                .param("draw", t_class("GmagickDraw"))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_imagick_draw", vec![e_this_prop("wand"), e_method_call(e_var("draw"), "_gmagickHandle", vec![])]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("GmagickException", vec![e_str("Gmagick::drawimage(): draw failed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("setImageBackgroundColor")
                .param_untyped("background")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_expr(e_call("elephc_imagick_fill", vec![e_this_prop("wand"), e_call("_gmagick_norm_color", vec![e_var("background")])])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("getNumberImages")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_count", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("getImageIndex")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_imagick_get_index", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("setImageIndex")
                .param("index", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_expr(e_call("elephc_imagick_set_index", vec![e_this_prop("wand"), e_var("index")])),
                    s_return(e_this()),
                ]),
        )
        .method(
            method("nextImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_imagick_next", vec![e_this_prop("wand")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("previousImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_imagick_previous", vec![e_this_prop("wand")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("hasNextImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_method_call(e_this(), "getImageIndex", vec![]), BinOp::Add, e_int(1)), BinOp::Lt, e_method_call(e_this(), "getNumberImages", vec![]))),
                ]),
        )
        .method(
            method("hasPreviousImage")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_method_call(e_this(), "getImageIndex", vec![]), BinOp::Gt, e_int(0))),
                ]),
        )
        .method(
            method("current")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_return(e_this()),
                ]),
        )
        .method(
            method("clear")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_imagick_clear", vec![e_this_prop("wand")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("destroy")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_imagick_destroy", vec![e_this_prop("wand")])),
                    s_prop_assign(e_this(), "wand", e_int(0)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_expr(e_call("elephc_imagick_destroy", vec![e_this_prop("wand")])),
                ]),
        )
        .method(
            method("getCopyright")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_str("elephc pure-Rust image bridge")),
                ]),
        )
        .method(
            method("getPackageName")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_str("elephc")),
                ]),
        )
        .method(
            method("getReleaseDate")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_str("")),
                ]),
        )
        .method(
            method("getQuantumDepth")
                .returns(t_array())
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("quantumDepthLong"), e_int(8)), (e_str("quantumString"), e_str("Q8"))])),
                ]),
        )
        .method(
            method("queryFormats")
                .param_default("pattern", TypeExpr::Str, e_str("*"))
                .returns(t_array())
                .body(vec![
                    s_assign("_u_pat", e_var("pattern")),
                    s_return(e_array(vec![e_str("BMP"), e_str("GIF"), e_str("JPEG"), e_str("PNG"), e_str("WEBP")])),
                ]),
        )
        .method(
            method("annotateImage")
                .param("draw", t_class("GmagickDraw"))
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .param("angle", TypeExpr::Float)
                .param("text", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_d", e_var("draw")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_a", e_var("angle")),
                    s_assign("_u_t", e_var("text")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::annotateimage() requires FreeType text, which is not supported in elephc")])),
                ]),
        )
        .method(
            method("charcoalImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_r", e_var("radius")),
                    s_assign("_u_s", e_var("sigma")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::charcoalimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("swirlImage")
                .param("degrees", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_d", e_var("degrees")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::swirlimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("oilPaintImage")
                .param("radius", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_r", e_var("radius")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::oilpaintimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("embossImage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_r", e_var("radius")),
                    s_assign("_u_s", e_var("sigma")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::embossimage() is not supported in elephc")])),
                ]),
        )
        // --- begin auto-generated API-surface throwing stubs (do not edit; regen via crates/elephc-image/tools/gen_image_api_stubs.py) ---
        .method(
            method("addnoiseimage")
                .param("noise_type", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_noise_type", e_var("noise_type")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::addnoiseimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("borderimage")
                .param("color", t_class("GmagickPixel"))
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_color", e_var("color")),
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::borderimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("chopimage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::chopimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("commentimage")
                .param("comment", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_comment", e_var("comment")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::commentimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("cropthumbnailimage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::cropthumbnailimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("cyclecolormapimage")
                .param("displace", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_displace", e_var("displace")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::cyclecolormapimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("deconstructimages")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::deconstructimages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("despeckleimage")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::despeckleimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("edgeimage")
                .param("radius", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::edgeimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("enhanceimage")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::enhanceimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("equalizeimage")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::equalizeimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("frameimage")
                .param("color", t_class("GmagickPixel"))
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("inner_bevel", TypeExpr::Int)
                .param("outer_bevel", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_color", e_var("color")),
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_inner_bevel", e_var("inner_bevel")),
                    s_assign("_u_outer_bevel", e_var("outer_bevel")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::frameimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("gammaimage")
                .param("gamma", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_gamma", e_var("gamma")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::gammaimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getfilename")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getfilename() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagebackgroundcolor")
                .returns(t_class("GmagickPixel"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagebackgroundcolor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimageblueprimary")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimageblueprimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagebordercolor")
                .returns(t_class("GmagickPixel"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagebordercolor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagechanneldepth")
                .param("channel_type", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("_u_channel_type", e_var("channel_type")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagechanneldepth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagecolors")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagecolors() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagecolorspace")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagecolorspace() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagecompose")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagecompose() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagedelay")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagedelay() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagedepth")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagedepth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagedispose")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagedispose() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimageextrema")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimageextrema() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagefilename")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagefilename() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagegamma")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagegamma() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagegreenprimary")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagegreenprimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagehistogram")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagehistogram() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimageinterlacescheme")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimageinterlacescheme() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimageiterations")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimageiterations() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagematte")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagematte() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagemattecolor")
                .returns(t_class("GmagickPixel"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagemattecolor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimageprofile")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimageprofile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimageredprimary")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimageredprimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagerenderingintent")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagerenderingintent() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimageresolution")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimageresolution() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagescene")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagescene() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagesignature")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagesignature() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagetype")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagetype() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimageunits")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimageunits() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getimagewhitepoint")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getimagewhitepoint() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getsamplingfactors")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getsamplingfactors() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getsize")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getsize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("getversion")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::getversion() is not supported in elephc")])),
                ]),
        )
        .method(
            method("implodeimage")
                .param("radius", TypeExpr::Float)
                .returns(t_mixed())
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::implodeimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("labelimage")
                .param("label", TypeExpr::Str)
                .returns(t_mixed())
                .body(vec![
                    s_assign("_u_label", e_var("label")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::labelimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("levelimage")
                .param("blackPoint", TypeExpr::Float)
                .param("gamma", TypeExpr::Float)
                .param("whitePoint", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(t_mixed())
                .body(vec![
                    s_assign("_u_blackPoint", e_var("blackPoint")),
                    s_assign("_u_gamma", e_var("gamma")),
                    s_assign("_u_whitePoint", e_var("whitePoint")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::levelimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("magnifyimage")
                .returns(t_mixed())
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::magnifyimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("mapimage")
                .param("gmagick", t_class("Gmagick"))
                .param("dither", TypeExpr::Bool)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_gmagick", e_var("gmagick")),
                    s_assign("_u_dither", e_var("dither")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::mapimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("medianfilterimage")
                .param("radius", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::medianfilterimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("minifyimage")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::minifyimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("motionblurimage")
                .param("radius", TypeExpr::Float)
                .param("sigma", TypeExpr::Float)
                .param("angle", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_assign("_u_sigma", e_var("sigma")),
                    s_assign("_u_angle", e_var("angle")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::motionblurimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("normalizeimage")
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::normalizeimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("profileimage")
                .param("name", TypeExpr::Str)
                .param("profile", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_assign("_u_profile", e_var("profile")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::profileimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("quantizeimage")
                .param("numColors", TypeExpr::Int)
                .param("colorspace", TypeExpr::Int)
                .param("treeDepth", TypeExpr::Int)
                .param("dither", TypeExpr::Bool)
                .param("measureError", TypeExpr::Bool)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_numColors", e_var("numColors")),
                    s_assign("_u_colorspace", e_var("colorspace")),
                    s_assign("_u_treeDepth", e_var("treeDepth")),
                    s_assign("_u_dither", e_var("dither")),
                    s_assign("_u_measureError", e_var("measureError")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::quantizeimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("quantizeimages")
                .param("numColors", TypeExpr::Int)
                .param("colorspace", TypeExpr::Int)
                .param("treeDepth", TypeExpr::Int)
                .param("dither", TypeExpr::Bool)
                .param("measureError", TypeExpr::Bool)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_numColors", e_var("numColors")),
                    s_assign("_u_colorspace", e_var("colorspace")),
                    s_assign("_u_treeDepth", e_var("treeDepth")),
                    s_assign("_u_dither", e_var("dither")),
                    s_assign("_u_measureError", e_var("measureError")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::quantizeimages() is not supported in elephc")])),
                ]),
        )
        .method(
            method("queryfontmetrics")
                .param("draw", t_class("GmagickDraw"))
                .param("text", TypeExpr::Str)
                .returns(t_array())
                .body(vec![
                    s_assign("_u_draw", e_var("draw")),
                    s_assign("_u_text", e_var("text")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::queryfontmetrics() is not supported in elephc")])),
                ]),
        )
        .method(
            method("queryfonts")
                .param_default("pattern", TypeExpr::Str, e_str("*"))
                .returns(t_array())
                .body(vec![
                    s_assign("_u_pattern", e_var("pattern")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::queryfonts() is not supported in elephc")])),
                ]),
        )
        .method(
            method("radialblurimage")
                .param("angle", TypeExpr::Float)
                .param_default("channel", TypeExpr::Int, e_int(0))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_angle", e_var("angle")),
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::radialblurimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("raiseimage")
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .param("raise", TypeExpr::Bool)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_width", e_var("width")),
                    s_assign("_u_height", e_var("height")),
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_assign("_u_raise", e_var("raise")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::raiseimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("read")
                .param("filename", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_filename", e_var("filename")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::read() is not supported in elephc")])),
                ]),
        )
        .method(
            method("readimagefile")
                .param_untyped("fp")
                .param_default("filename", TypeExpr::Str, e_str(""))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_fp", e_var("fp")),
                    s_assign("_u_filename", e_var("filename")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::readimagefile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("reducenoiseimage")
                .param("radius", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::reducenoiseimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("removeimage")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::removeimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("removeimageprofile")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::removeimageprofile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("resampleimage")
                .param("xResolution", TypeExpr::Float)
                .param("yResolution", TypeExpr::Float)
                .param("filter", TypeExpr::Int)
                .param("blur", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_xResolution", e_var("xResolution")),
                    s_assign("_u_yResolution", e_var("yResolution")),
                    s_assign("_u_filter", e_var("filter")),
                    s_assign("_u_blur", e_var("blur")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::resampleimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("rollimage")
                .param("x", TypeExpr::Int)
                .param("y", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::rollimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("separateimagechannel")
                .param("channel", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::separateimagechannel() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setfilename")
                .param("filename", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_filename", e_var("filename")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setfilename() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimageblueprimary")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimageblueprimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagebordercolor")
                .param("color", t_class("GmagickPixel"))
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_color", e_var("color")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagebordercolor() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagechanneldepth")
                .param("channel", TypeExpr::Int)
                .param("depth", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_channel", e_var("channel")),
                    s_assign("_u_depth", e_var("depth")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagechanneldepth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagecolorspace")
                .param("colorspace", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_colorspace", e_var("colorspace")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagecolorspace() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagecompose")
                .param("composite", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_composite", e_var("composite")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagecompose() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagedelay")
                .param("delay", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_delay", e_var("delay")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagedelay() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagedepth")
                .param("depth", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_depth", e_var("depth")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagedepth() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagedispose")
                .param("disposeType", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_disposeType", e_var("disposeType")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagedispose() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagefilename")
                .param("filename", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_filename", e_var("filename")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagefilename() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagegamma")
                .param("gamma", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_gamma", e_var("gamma")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagegamma() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagegreenprimary")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagegreenprimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimageinterlacescheme")
                .param("interlace", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_interlace", e_var("interlace")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimageinterlacescheme() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimageiterations")
                .param("iterations", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_iterations", e_var("iterations")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimageiterations() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimageprofile")
                .param("name", TypeExpr::Str)
                .param("profile", TypeExpr::Str)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_name", e_var("name")),
                    s_assign("_u_profile", e_var("profile")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimageprofile() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimageredprimary")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimageredprimary() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagerenderingintent")
                .param("rendering_intent", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_rendering_intent", e_var("rendering_intent")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagerenderingintent() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimageresolution")
                .param("xResolution", TypeExpr::Float)
                .param("yResolution", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_xResolution", e_var("xResolution")),
                    s_assign("_u_yResolution", e_var("yResolution")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimageresolution() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagescene")
                .param("scene", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_scene", e_var("scene")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagescene() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagetype")
                .param("imgType", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_imgType", e_var("imgType")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagetype() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimageunits")
                .param("resolution", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_resolution", e_var("resolution")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimageunits() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setimagewhitepoint")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_x", e_var("x")),
                    s_assign("_u_y", e_var("y")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setimagewhitepoint() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setsamplingfactors")
                .param("factors", t_array())
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_factors", e_var("factors")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setsamplingfactors() is not supported in elephc")])),
                ]),
        )
        .method(
            method("setsize")
                .param("columns", TypeExpr::Int)
                .param("rows", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_columns", e_var("columns")),
                    s_assign("_u_rows", e_var("rows")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::setsize() is not supported in elephc")])),
                ]),
        )
        .method(
            method("shearimage")
                .param_untyped("color")
                .param("xShear", TypeExpr::Float)
                .param("yShear", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_color", e_var("color")),
                    s_assign("_u_xShear", e_var("xShear")),
                    s_assign("_u_yShear", e_var("yShear")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::shearimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("solarizeimage")
                .param("threshold", TypeExpr::Int)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_threshold", e_var("threshold")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::solarizeimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("spreadimage")
                .param("radius", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_radius", e_var("radius")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::spreadimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("stripimage")
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::stripimage() is not supported in elephc")])),
                ]),
        )
        .method(
            method("trimimage")
                .param("fuzz", TypeExpr::Float)
                .returns(t_class("Gmagick"))
                .body(vec![
                    s_assign("_u_fuzz", e_var("fuzz")),
                    s_throw(e_new("GmagickException", vec![e_str("Gmagick::trimimage() is not supported in elephc")])),
                ]),
        )
        // --- end auto-generated API-surface stubs ---
        .build()
}

/// `CairoException` — transcribed from the PHP form.
fn decl_class_cairoexception() -> Stmt {
    class("CairoException")
        .extends("Exception")
        .build()
}

/// `CairoFormat` — transcribed from the PHP form.
fn decl_class_cairoformat() -> Stmt {
    class("CairoFormat")
        .constant("ARGB32", e_int(0))
        .constant("RGB24", e_int(1))
        .constant("A8", e_int(2))
        .constant("A1", e_int(3))
        .build()
}

/// `CairoAntialias` — transcribed from the PHP form.
fn decl_class_cairoantialias() -> Stmt {
    class("CairoAntialias")
        .constant("DEFAULT", e_int(0))
        .constant("NONE", e_int(1))
        .constant("GRAY", e_int(2))
        .constant("SUBPIXEL", e_int(3))
        .build()
}

/// `CairoLineCap` — transcribed from the PHP form.
fn decl_class_cairolinecap() -> Stmt {
    class("CairoLineCap")
        .constant("BUTT", e_int(0))
        .constant("ROUND", e_int(1))
        .constant("SQUARE", e_int(2))
        .build()
}

/// `CairoLineJoin` — transcribed from the PHP form.
fn decl_class_cairolinejoin() -> Stmt {
    class("CairoLineJoin")
        .constant("MITER", e_int(0))
        .constant("ROUND", e_int(1))
        .constant("BEVEL", e_int(2))
        .build()
}

/// `CairoFillRule` — transcribed from the PHP form.
fn decl_class_cairofillrule() -> Stmt {
    class("CairoFillRule")
        .constant("WINDING", e_int(0))
        .constant("EVEN_ODD", e_int(1))
        .build()
}

/// `CairoFontSlant` — transcribed from the PHP form.
fn decl_class_cairofontslant() -> Stmt {
    class("CairoFontSlant")
        .constant("NORMAL", e_int(0))
        .constant("ITALIC", e_int(1))
        .constant("OBLIQUE", e_int(2))
        .build()
}

/// `CairoFontWeight` — transcribed from the PHP form.
fn decl_class_cairofontweight() -> Stmt {
    class("CairoFontWeight")
        .constant("NORMAL", e_int(0))
        .constant("BOLD", e_int(1))
        .build()
}

/// `_cairo_fx` — transcribed from the PHP form.
fn decl_fn_cairo_fx() -> Stmt {
    function("_cairo_fx")
        .param_untyped("v")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_cast(CastType::Int, e_call("round", vec![e_binop(e_cast(CastType::Float, e_var("v")), BinOp::Mul, e_float(1000.0))]))),
        ])
        .build()
}

/// `_cairo_pack` — transcribed from the PHP form.
fn decl_fn_cairo_pack() -> Stmt {
    function("_cairo_pack")
        .param_untyped("x")
        .param_untyped("y")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("_imagick_pack2", vec![e_call("_cairo_fx", vec![e_var("x")]), e_call("_cairo_fx", vec![e_var("y")])])),
        ])
        .build()
}

/// `_cairo_clamp8` — transcribed from the PHP form.
fn decl_fn_cairo_clamp8() -> Stmt {
    function("_cairo_clamp8")
        .param("v", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_var("v"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("v"), BinOp::Gt, e_int(255)),
                vec![
                    s_return(e_int(255)),
                ],
                vec![],
                None,
            ),
            s_return(e_var("v")),
        ])
        .build()
}

/// `_cairo_color` — transcribed from the PHP form.
fn decl_fn_cairo_color() -> Stmt {
    function("_cairo_color")
        .param_untyped("r")
        .param_untyped("g")
        .param_untyped("b")
        .param_untyped("a")
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_ri", e_call("_cairo_clamp8", vec![e_cast(CastType::Int, e_call("round", vec![e_binop(e_cast(CastType::Float, e_var("r")), BinOp::Mul, e_float(255.0))]))])),
            s_assign("_gi", e_call("_cairo_clamp8", vec![e_cast(CastType::Int, e_call("round", vec![e_binop(e_cast(CastType::Float, e_var("g")), BinOp::Mul, e_float(255.0))]))])),
            s_assign("_bi", e_call("_cairo_clamp8", vec![e_cast(CastType::Int, e_call("round", vec![e_binop(e_cast(CastType::Float, e_var("b")), BinOp::Mul, e_float(255.0))]))])),
            s_assign("_ai", e_call("_cairo_clamp8", vec![e_cast(CastType::Int, e_call("round", vec![e_binop(e_cast(CastType::Float, e_var("a")), BinOp::Mul, e_float(255.0))]))])),
            s_return(e_binop(e_binop(e_binop(e_binop(e_var("_ri"), BinOp::ShiftLeft, e_int(24)), BinOp::BitOr, e_binop(e_var("_gi"), BinOp::ShiftLeft, e_int(16))), BinOp::BitOr, e_binop(e_var("_bi"), BinOp::ShiftLeft, e_int(8))), BinOp::BitOr, e_var("_ai"))),
        ])
        .build()
}

/// `CairoSurface` — transcribed from the PHP form.
fn decl_class_cairosurface() -> Stmt {
    class("CairoSurface")
        .build()
}

/// `CairoImageSurface` — transcribed from the PHP form.
fn decl_class_cairoimagesurface() -> Stmt {
    class("CairoImageSurface")
        .extends("CairoSurface")
        .prop("surface", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param("format", TypeExpr::Int)
                .param("width", TypeExpr::Int)
                .param("height", TypeExpr::Int)
                .body(vec![
                    s_assign("_u_fmt", e_var("format")),
                    s_prop_assign(e_this(), "surface", e_call("elephc_cairo_surface_create", vec![e_var("width"), e_var("height")])),
                    s_if(
                        e_binop(e_this_prop("surface"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("CairoException", vec![e_str("CairoImageSurface: invalid dimensions")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("_surfaceHandle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("surface")),
                ]),
        )
        .method(
            method("getWidth")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_cairo_surface_width", vec![e_this_prop("surface")])),
                ]),
        )
        .method(
            method("getHeight")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_cairo_surface_height", vec![e_this_prop("surface")])),
                ]),
        )
        .method(
            method("getFormat")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("status")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("flush")
                .returns(TypeExpr::Void),
        )
        .method(
            method("finish")
                .returns(TypeExpr::Void),
        )
        .method(
            method("writeToPng")
                .param("file", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_cairo_surface_write_png", vec![e_this_prop("surface"), e_var("file")]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_throw(e_new("CairoException", vec![e_binop(e_binop(e_str("CairoImageSurface::writeToPng(): unable to write '"), BinOp::Concat, e_var("file")), BinOp::Concat, e_str("'"))])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("createFromPng")
                .static_()
                .param("file", TypeExpr::Str)
                .returns(t_class("CairoImageSurface"))
                .body(vec![
                    s_assign("h", e_call("elephc_cairo_surface_create_from_png", vec![e_var("file")])),
                    s_if(
                        e_binop(e_var("h"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("CairoException", vec![e_binop(e_binop(e_str("CairoImageSurface::createFromPng(): unable to load '"), BinOp::Concat, e_var("file")), BinOp::Concat, e_str("'"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("s", e_new("CairoImageSurface", vec![e_class_const("CairoFormat", "ARGB32"), e_int(1), e_int(1)])),
                    s_expr(e_call("elephc_cairo_surface_destroy", vec![e_prop(e_var("s"), "surface")])),
                    s_prop_assign(e_var("s"), "surface", e_var("h")),
                    s_return(e_var("s")),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_expr(e_call("elephc_cairo_surface_destroy", vec![e_this_prop("surface")])),
                ]),
        )
        .build()
}

/// `CairoPdfSurface` — transcribed from the PHP form.
fn decl_class_cairopdfsurface() -> Stmt {
    class("CairoPdfSurface")
        .extends("CairoSurface")
        .method(
            method("__construct")
                .param("file", TypeExpr::Str)
                .param_untyped("width")
                .param_untyped("height")
                .body(vec![
                    s_assign("_u_f", e_var("file")),
                    s_assign("_u_w", e_var("width")),
                    s_assign("_u_h", e_var("height")),
                    s_throw(e_new("CairoException", vec![e_str("CairoPdfSurface is not supported in elephc (no pure-Rust PDF surface)")])),
                ]),
        )
        .build()
}

/// `CairoPsSurface` — transcribed from the PHP form.
fn decl_class_cairopssurface() -> Stmt {
    class("CairoPsSurface")
        .extends("CairoSurface")
        .method(
            method("__construct")
                .param("file", TypeExpr::Str)
                .param_untyped("width")
                .param_untyped("height")
                .body(vec![
                    s_assign("_u_f", e_var("file")),
                    s_assign("_u_w", e_var("width")),
                    s_assign("_u_h", e_var("height")),
                    s_throw(e_new("CairoException", vec![e_str("CairoPsSurface is not supported in elephc (no pure-Rust PostScript surface)")])),
                ]),
        )
        .build()
}

/// `CairoSvgSurface` — transcribed from the PHP form.
fn decl_class_cairosvgsurface() -> Stmt {
    class("CairoSvgSurface")
        .extends("CairoSurface")
        .method(
            method("__construct")
                .param("file", TypeExpr::Str)
                .param_untyped("width")
                .param_untyped("height")
                .body(vec![
                    s_assign("_u_f", e_var("file")),
                    s_assign("_u_w", e_var("width")),
                    s_assign("_u_h", e_var("height")),
                    s_throw(e_new("CairoException", vec![e_str("CairoSvgSurface is not supported in elephc (no pure-Rust SVG surface)")])),
                ]),
        )
        .build()
}

/// `CairoPattern` — transcribed from the PHP form.
fn decl_class_cairopattern() -> Stmt {
    class("CairoPattern")
        .prop("pattern", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("_patternHandle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("pattern")),
                ]),
        )
        .method(
            method("status")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_expr(e_call("elephc_cairo_pattern_destroy", vec![e_this_prop("pattern")])),
                ]),
        )
        .build()
}

/// `CairoSolidPattern` — transcribed from the PHP form.
fn decl_class_cairosolidpattern() -> Stmt {
    class("CairoSolidPattern")
        .extends("CairoPattern")
        .method(
            method("__construct")
                .param("r", TypeExpr::Float)
                .param("g", TypeExpr::Float)
                .param("b", TypeExpr::Float)
                .param_default("a", TypeExpr::Float, e_float(1.0))
                .body(vec![
                    s_prop_assign(e_this(), "pattern", e_call("elephc_cairo_pattern_create_rgba", vec![e_call("_cairo_color", vec![e_var("r"), e_var("g"), e_var("b"), e_var("a")])])),
                ]),
        )
        .method(
            method("createRgb")
                .static_()
                .param("r", TypeExpr::Float)
                .param("g", TypeExpr::Float)
                .param("b", TypeExpr::Float)
                .returns(t_class("CairoSolidPattern"))
                .body(vec![
                    s_return(e_new("CairoSolidPattern", vec![e_var("r"), e_var("g"), e_var("b"), e_float(1.0)])),
                ]),
        )
        .method(
            method("createRgba")
                .static_()
                .param("r", TypeExpr::Float)
                .param("g", TypeExpr::Float)
                .param("b", TypeExpr::Float)
                .param("a", TypeExpr::Float)
                .returns(t_class("CairoSolidPattern"))
                .body(vec![
                    s_return(e_new("CairoSolidPattern", vec![e_var("r"), e_var("g"), e_var("b"), e_var("a")])),
                ]),
        )
        .build()
}

/// `CairoGradientPattern` — transcribed from the PHP form.
fn decl_class_cairogradientpattern() -> Stmt {
    class("CairoGradientPattern")
        .extends("CairoPattern")
        .method(
            method("addColorStopRgb")
                .param("offset", TypeExpr::Float)
                .param("r", TypeExpr::Float)
                .param("g", TypeExpr::Float)
                .param("b", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_pattern_add_color_stop_rgba", vec![e_this_prop("pattern"), e_call("_cairo_fx", vec![e_var("offset")]), e_call("_cairo_color", vec![e_var("r"), e_var("g"), e_var("b"), e_float(1.0)])])),
                ]),
        )
        .method(
            method("addColorStopRgba")
                .param("offset", TypeExpr::Float)
                .param("r", TypeExpr::Float)
                .param("g", TypeExpr::Float)
                .param("b", TypeExpr::Float)
                .param("a", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_pattern_add_color_stop_rgba", vec![e_this_prop("pattern"), e_call("_cairo_fx", vec![e_var("offset")]), e_call("_cairo_color", vec![e_var("r"), e_var("g"), e_var("b"), e_var("a")])])),
                ]),
        )
        .build()
}

/// `CairoLinearGradient` — transcribed from the PHP form.
fn decl_class_cairolineargradient() -> Stmt {
    class("CairoLinearGradient")
        .extends("CairoGradientPattern")
        .method(
            method("__construct")
                .param("x0", TypeExpr::Float)
                .param("y0", TypeExpr::Float)
                .param("x1", TypeExpr::Float)
                .param("y1", TypeExpr::Float)
                .body(vec![
                    s_prop_assign(e_this(), "pattern", e_call("elephc_cairo_pattern_create_linear", vec![e_call("_cairo_pack", vec![e_var("x0"), e_var("y0")]), e_call("_cairo_pack", vec![e_var("x1"), e_var("y1")])])),
                ]),
        )
        .build()
}

/// `CairoRadialGradient` — transcribed from the PHP form.
fn decl_class_cairoradialgradient() -> Stmt {
    class("CairoRadialGradient")
        .extends("CairoGradientPattern")
        .method(
            method("__construct")
                .param("cx0", TypeExpr::Float)
                .param("cy0", TypeExpr::Float)
                .param("radius0", TypeExpr::Float)
                .param("cx1", TypeExpr::Float)
                .param("cy1", TypeExpr::Float)
                .param("radius1", TypeExpr::Float)
                .body(vec![
                    s_prop_assign(e_this(), "pattern", e_call("elephc_cairo_pattern_create_radial", vec![e_call("_cairo_pack", vec![e_var("cx0"), e_var("cy0")]), e_call("_cairo_fx", vec![e_var("radius0")]), e_call("_cairo_pack", vec![e_var("cx1"), e_var("cy1")]), e_call("_cairo_fx", vec![e_var("radius1")])])),
                ]),
        )
        .build()
}

/// `CairoSurfacePattern` — transcribed from the PHP form.
fn decl_class_cairosurfacepattern() -> Stmt {
    class("CairoSurfacePattern")
        .extends("CairoPattern")
        .method(
            method("__construct")
                .param("surface", t_class("CairoImageSurface"))
                .body(vec![
                    s_assign("_u", e_var("surface")),
                    s_throw(e_new("CairoException", vec![e_str("CairoSurfacePattern is not supported in elephc")])),
                ]),
        )
        .build()
}

/// `CairoMatrix` — transcribed from the PHP form.
fn decl_class_cairomatrix() -> Stmt {
    class("CairoMatrix")
        .prop("xx", TypeExpr::Float, Some(e_float(1.0)))
        .prop("yx", TypeExpr::Float, Some(e_float(0.0)))
        .prop("xy", TypeExpr::Float, Some(e_float(0.0)))
        .prop("yy", TypeExpr::Float, Some(e_float(1.0)))
        .prop("x0", TypeExpr::Float, Some(e_float(0.0)))
        .prop("y0", TypeExpr::Float, Some(e_float(0.0)))
        .method(
            method("__construct")
                .param_default("xx", TypeExpr::Float, e_float(1.0))
                .param_default("yx", TypeExpr::Float, e_float(0.0))
                .param_default("xy", TypeExpr::Float, e_float(0.0))
                .param_default("yy", TypeExpr::Float, e_float(1.0))
                .param_default("x0", TypeExpr::Float, e_float(0.0))
                .param_default("y0", TypeExpr::Float, e_float(0.0))
                .body(vec![
                    s_prop_assign(e_this(), "xx", e_var("xx")),
                    s_prop_assign(e_this(), "yx", e_var("yx")),
                    s_prop_assign(e_this(), "xy", e_var("xy")),
                    s_prop_assign(e_this(), "yy", e_var("yy")),
                    s_prop_assign(e_this(), "x0", e_var("x0")),
                    s_prop_assign(e_this(), "y0", e_var("y0")),
                ]),
        )
        .method(
            method("initIdentity")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "xx", e_float(1.0)),
                    s_prop_assign(e_this(), "yx", e_float(0.0)),
                    s_prop_assign(e_this(), "xy", e_float(0.0)),
                    s_prop_assign(e_this(), "yy", e_float(1.0)),
                    s_prop_assign(e_this(), "x0", e_float(0.0)),
                    s_prop_assign(e_this(), "y0", e_float(0.0)),
                ]),
        )
        .method(
            method("initTranslate")
                .param("tx", TypeExpr::Float)
                .param("ty", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "xx", e_float(1.0)),
                    s_prop_assign(e_this(), "yx", e_float(0.0)),
                    s_prop_assign(e_this(), "xy", e_float(0.0)),
                    s_prop_assign(e_this(), "yy", e_float(1.0)),
                    s_prop_assign(e_this(), "x0", e_var("tx")),
                    s_prop_assign(e_this(), "y0", e_var("ty")),
                ]),
        )
        .method(
            method("initScale")
                .param("sx", TypeExpr::Float)
                .param("sy", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "xx", e_var("sx")),
                    s_prop_assign(e_this(), "yx", e_float(0.0)),
                    s_prop_assign(e_this(), "xy", e_float(0.0)),
                    s_prop_assign(e_this(), "yy", e_var("sy")),
                    s_prop_assign(e_this(), "x0", e_float(0.0)),
                    s_prop_assign(e_this(), "y0", e_float(0.0)),
                ]),
        )
        .method(
            method("initRotate")
                .param("radians", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_c", e_call("cos", vec![e_var("radians")])),
                    s_assign("_s", e_call("sin", vec![e_var("radians")])),
                    s_prop_assign(e_this(), "xx", e_var("_c")),
                    s_prop_assign(e_this(), "yx", e_var("_s")),
                    s_prop_assign(e_this(), "xy", e_neg(e_var("_s"))),
                    s_prop_assign(e_this(), "yy", e_var("_c")),
                    s_prop_assign(e_this(), "x0", e_float(0.0)),
                    s_prop_assign(e_this(), "y0", e_float(0.0)),
                ]),
        )
        .method(
            method("transformPoint")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(t_array())
                .body(vec![
                    s_assign("_fx", e_var("x")),
                    s_assign("_fy", e_var("y")),
                    s_return(e_array_assoc(vec![(e_str("x"), e_binop(e_binop(e_binop(e_this_prop("xx"), BinOp::Mul, e_var("_fx")), BinOp::Add, e_binop(e_this_prop("xy"), BinOp::Mul, e_var("_fy"))), BinOp::Add, e_this_prop("x0"))), (e_str("y"), e_binop(e_binop(e_binop(e_this_prop("yx"), BinOp::Mul, e_var("_fx")), BinOp::Add, e_binop(e_this_prop("yy"), BinOp::Mul, e_var("_fy"))), BinOp::Add, e_this_prop("y0")))])),
                ]),
        )
        .build()
}

/// `CairoContext` — transcribed from the PHP form.
fn decl_class_cairocontext() -> Stmt {
    class("CairoContext")
        .prop("ctx", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param("surface", t_class("CairoImageSurface"))
                .body(vec![
                    s_prop_assign(e_this(), "ctx", e_call("elephc_cairo_create", vec![e_method_call(e_var("surface"), "_surfaceHandle", vec![])])),
                    s_if(
                        e_binop(e_this_prop("ctx"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("CairoException", vec![e_str("CairoContext: invalid surface")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("save")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_save", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("restore")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_restore", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("setSourceRgb")
                .param("r", TypeExpr::Float)
                .param("g", TypeExpr::Float)
                .param("b", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_set_source_rgba", vec![e_this_prop("ctx"), e_call("_cairo_color", vec![e_var("r"), e_var("g"), e_var("b"), e_float(1.0)])])),
                ]),
        )
        .method(
            method("setSourceRgba")
                .param("r", TypeExpr::Float)
                .param("g", TypeExpr::Float)
                .param("b", TypeExpr::Float)
                .param("a", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_set_source_rgba", vec![e_this_prop("ctx"), e_call("_cairo_color", vec![e_var("r"), e_var("g"), e_var("b"), e_var("a")])])),
                ]),
        )
        .method(
            method("setSource")
                .param("pattern", t_class("CairoPattern"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_set_source_pattern", vec![e_this_prop("ctx"), e_method_call(e_var("pattern"), "_patternHandle", vec![])])),
                ]),
        )
        .method(
            method("setLineWidth")
                .param("width", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_set_line_width", vec![e_this_prop("ctx"), e_call("_cairo_fx", vec![e_var("width")])])),
                ]),
        )
        .method(
            method("setLineCap")
                .param("cap", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_set_line_cap", vec![e_this_prop("ctx"), e_var("cap")])),
                ]),
        )
        .method(
            method("setLineJoin")
                .param("join", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_set_line_join", vec![e_this_prop("ctx"), e_var("join")])),
                ]),
        )
        .method(
            method("setFillRule")
                .param("rule", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_set_fill_rule", vec![e_this_prop("ctx"), e_var("rule")])),
                ]),
        )
        .method(
            method("moveTo")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_move_to", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_var("x"), e_var("y")])])),
                ]),
        )
        .method(
            method("lineTo")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_line_to", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_var("x"), e_var("y")])])),
                ]),
        )
        .method(
            method("curveTo")
                .param("x1", TypeExpr::Float)
                .param("y1", TypeExpr::Float)
                .param("x2", TypeExpr::Float)
                .param("y2", TypeExpr::Float)
                .param("x3", TypeExpr::Float)
                .param("y3", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_curve_to", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_var("x1"), e_var("y1")]), e_call("_cairo_pack", vec![e_var("x2"), e_var("y2")]), e_call("_cairo_pack", vec![e_var("x3"), e_var("y3")])])),
                ]),
        )
        .method(
            method("rectangle")
                .param("x", TypeExpr::Float)
                .param("y", TypeExpr::Float)
                .param("width", TypeExpr::Float)
                .param("height", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_rectangle", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_var("x"), e_var("y")]), e_call("_cairo_pack", vec![e_var("width"), e_var("height")])])),
                ]),
        )
        .method(
            method("arc")
                .param("xc", TypeExpr::Float)
                .param("yc", TypeExpr::Float)
                .param("radius", TypeExpr::Float)
                .param("angle1", TypeExpr::Float)
                .param("angle2", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_arc", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_var("xc"), e_var("yc")]), e_call("_cairo_fx", vec![e_var("radius")]), e_call("_cairo_pack", vec![e_var("angle1"), e_var("angle2")])])),
                ]),
        )
        .method(
            method("arcNegative")
                .param("xc", TypeExpr::Float)
                .param("yc", TypeExpr::Float)
                .param("radius", TypeExpr::Float)
                .param("angle1", TypeExpr::Float)
                .param("angle2", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_arc_negative", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_var("xc"), e_var("yc")]), e_call("_cairo_fx", vec![e_var("radius")]), e_call("_cairo_pack", vec![e_var("angle1"), e_var("angle2")])])),
                ]),
        )
        .method(
            method("closePath")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_close_path", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("newPath")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_new_path", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("newSubPath")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_new_sub_path", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("paint")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_paint", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("fill")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_fill", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("fillPreserve")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_fill_preserve", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("stroke")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_stroke", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("strokePreserve")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_stroke_preserve", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("translate")
                .param("tx", TypeExpr::Float)
                .param("ty", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_translate", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_var("tx"), e_var("ty")])])),
                ]),
        )
        .method(
            method("scale")
                .param("sx", TypeExpr::Float)
                .param("sy", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_scale", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_var("sx"), e_var("sy")])])),
                ]),
        )
        .method(
            method("rotate")
                .param("angle", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_rotate", vec![e_this_prop("ctx"), e_call("_cairo_fx", vec![e_var("angle")])])),
                ]),
        )
        .method(
            method("setMatrix")
                .param("matrix", t_class("CairoMatrix"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_set_matrix", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_prop(e_var("matrix"), "xx"), e_prop(e_var("matrix"), "yx")]), e_call("_cairo_pack", vec![e_prop(e_var("matrix"), "xy"), e_prop(e_var("matrix"), "yy")]), e_call("_cairo_pack", vec![e_prop(e_var("matrix"), "x0"), e_prop(e_var("matrix"), "y0")])])),
                ]),
        )
        .method(
            method("transform")
                .param("matrix", t_class("CairoMatrix"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_transform", vec![e_this_prop("ctx"), e_call("_cairo_pack", vec![e_prop(e_var("matrix"), "xx"), e_prop(e_var("matrix"), "yx")]), e_call("_cairo_pack", vec![e_prop(e_var("matrix"), "xy"), e_prop(e_var("matrix"), "yy")]), e_call("_cairo_pack", vec![e_prop(e_var("matrix"), "x0"), e_prop(e_var("matrix"), "y0")])])),
                ]),
        )
        .method(
            method("identityMatrix")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_cairo_identity_matrix", vec![e_this_prop("ctx")])),
                ]),
        )
        .method(
            method("getCurrentPoint")
                .returns(t_array())
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("x"), e_binop(e_call("elephc_cairo_get_current_point_x", vec![e_this_prop("ctx")]), BinOp::Div, e_float(1000.0))), (e_str("y"), e_binop(e_call("elephc_cairo_get_current_point_y", vec![e_this_prop("ctx")]), BinOp::Div, e_float(1000.0)))])),
                ]),
        )
        .method(
            method("selectFontFace")
                .param("family", TypeExpr::Str)
                .param_default("slant", TypeExpr::Int, e_int(0))
                .param_default("weight", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_u_f", e_var("family")),
                    s_assign("_u_s", e_var("slant")),
                    s_assign("_u_w", e_var("weight")),
                ]),
        )
        .method(
            method("setFontSize")
                .param("size", TypeExpr::Float)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_u", e_var("size")),
                ]),
        )
        .method(
            method("showText")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_u", e_var("text")),
                    s_throw(e_new("CairoException", vec![e_str("CairoContext::showText() requires FreeType, which is not supported in elephc")])),
                ]),
        )
        .method(
            method("textExtents")
                .param("text", TypeExpr::Str)
                .returns(t_array())
                .body(vec![
                    s_assign("_u", e_var("text")),
                    s_throw(e_new("CairoException", vec![e_str("CairoContext::textExtents() requires FreeType, which is not supported in elephc")])),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_expr(e_call("elephc_cairo_destroy", vec![e_this_prop("ctx")])),
                ]),
        )
        .build()
}

/// `CairoFontFace` — transcribed from the PHP form.
fn decl_class_cairofontface() -> Stmt {
    class("CairoFontFace")
        .method(
            method("status")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(0)),
                ]),
        )
        .build()
}

/// `CairoToyFontFace` — transcribed from the PHP form.
fn decl_class_cairotoyfontface() -> Stmt {
    class("CairoToyFontFace")
        .extends("CairoFontFace")
        .method(
            method("__construct")
                .param("family", TypeExpr::Str)
                .param_default("slant", TypeExpr::Int, e_int(0))
                .param_default("weight", TypeExpr::Int, e_int(0))
                .body(vec![
                    s_assign("_u_f", e_var("family")),
                    s_assign("_u_s", e_var("slant")),
                    s_assign("_u_w", e_var("weight")),
                    s_throw(e_new("CairoException", vec![e_str("CairoToyFontFace requires FreeType, which is not supported in elephc")])),
                ]),
        )
        .build()
}

/// `CairoFontOptions` — transcribed from the PHP form.
fn decl_class_cairofontoptions() -> Stmt {
    class("CairoFontOptions")
        .method(
            method("status")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(0)),
                ]),
        )
        .build()
}

/// `CairoScaledFont` — transcribed from the PHP form.
fn decl_class_cairoscaledfont() -> Stmt {
    class("CairoScaledFont")
        .method(
            method("__construct")
                .param("fontFace", t_class("CairoFontFace"))
                .param("matrix", t_class("CairoMatrix"))
                .param("ctm", t_class("CairoMatrix"))
                .param("options", t_class("CairoFontOptions"))
                .body(vec![
                    s_assign("_u_a", e_var("fontFace")),
                    s_assign("_u_b", e_var("matrix")),
                    s_assign("_u_c", e_var("ctm")),
                    s_assign("_u_d", e_var("options")),
                    s_throw(e_new("CairoException", vec![e_str("CairoScaledFont requires FreeType, which is not supported in elephc")])),
                ]),
        )
        .build()
}

/// `CairoPath` — transcribed from the PHP form.
fn decl_class_cairopath() -> Stmt {
    class("CairoPath")
        .build()
}

/// `cairo_image_surface_create` — transcribed from the PHP form.
fn decl_fn_cairo_image_surface_create() -> Stmt {
    function("cairo_image_surface_create")
        .param("format", TypeExpr::Int)
        .param("width", TypeExpr::Int)
        .param("height", TypeExpr::Int)
        .returns(t_class("CairoImageSurface"))
        .body(vec![
            s_return(e_new("CairoImageSurface", vec![e_var("format"), e_var("width"), e_var("height")])),
        ])
        .build()
}

/// `cairo_image_surface_create_from_png` — transcribed from the PHP form.
fn decl_fn_cairo_image_surface_create_from_png() -> Stmt {
    function("cairo_image_surface_create_from_png")
        .param("filename", TypeExpr::Str)
        .returns(t_class("CairoImageSurface"))
        .body(vec![
            s_return(e_static_call("CairoImageSurface", "createFromPng", vec![e_var("filename")])),
        ])
        .build()
}

/// `cairo_image_surface_get_width` — transcribed from the PHP form.
fn decl_fn_cairo_image_surface_get_width() -> Stmt {
    function("cairo_image_surface_get_width")
        .param("surface", t_class("CairoImageSurface"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_var("surface"), "getWidth", vec![])),
        ])
        .build()
}

/// `cairo_image_surface_get_height` — transcribed from the PHP form.
fn decl_fn_cairo_image_surface_get_height() -> Stmt {
    function("cairo_image_surface_get_height")
        .param("surface", t_class("CairoImageSurface"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_var("surface"), "getHeight", vec![])),
        ])
        .build()
}

/// `cairo_surface_write_to_png` — transcribed from the PHP form.
fn decl_fn_cairo_surface_write_to_png() -> Stmt {
    function("cairo_surface_write_to_png")
        .param("surface", t_class("CairoImageSurface"))
        .param("filename", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("surface"), "writeToPng", vec![e_var("filename")])),
        ])
        .build()
}

/// `cairo_create` — transcribed from the PHP form.
fn decl_fn_cairo_create() -> Stmt {
    function("cairo_create")
        .param("surface", t_class("CairoImageSurface"))
        .returns(t_class("CairoContext"))
        .body(vec![
            s_return(e_new("CairoContext", vec![e_var("surface")])),
        ])
        .build()
}

/// `cairo_save` — transcribed from the PHP form.
fn decl_fn_cairo_save() -> Stmt {
    function("cairo_save")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "save", vec![])),
        ])
        .build()
}

/// `cairo_restore` — transcribed from the PHP form.
fn decl_fn_cairo_restore() -> Stmt {
    function("cairo_restore")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "restore", vec![])),
        ])
        .build()
}

/// `cairo_set_source_rgb` — transcribed from the PHP form.
fn decl_fn_cairo_set_source_rgb() -> Stmt {
    function("cairo_set_source_rgb")
        .param("context", t_class("CairoContext"))
        .param("red", TypeExpr::Float)
        .param("green", TypeExpr::Float)
        .param("blue", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "setSourceRgb", vec![e_var("red"), e_var("green"), e_var("blue")])),
        ])
        .build()
}

/// `cairo_set_source_rgba` — transcribed from the PHP form.
fn decl_fn_cairo_set_source_rgba() -> Stmt {
    function("cairo_set_source_rgba")
        .param("context", t_class("CairoContext"))
        .param("red", TypeExpr::Float)
        .param("green", TypeExpr::Float)
        .param("blue", TypeExpr::Float)
        .param("alpha", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "setSourceRgba", vec![e_var("red"), e_var("green"), e_var("blue"), e_var("alpha")])),
        ])
        .build()
}

/// `cairo_set_source` — transcribed from the PHP form.
fn decl_fn_cairo_set_source() -> Stmt {
    function("cairo_set_source")
        .param("context", t_class("CairoContext"))
        .param("pattern", t_class("CairoPattern"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "setSource", vec![e_var("pattern")])),
        ])
        .build()
}

/// `cairo_set_line_width` — transcribed from the PHP form.
fn decl_fn_cairo_set_line_width() -> Stmt {
    function("cairo_set_line_width")
        .param("context", t_class("CairoContext"))
        .param("width", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "setLineWidth", vec![e_var("width")])),
        ])
        .build()
}

/// `cairo_set_line_cap` — transcribed from the PHP form.
fn decl_fn_cairo_set_line_cap() -> Stmt {
    function("cairo_set_line_cap")
        .param("context", t_class("CairoContext"))
        .param("lineCap", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "setLineCap", vec![e_var("lineCap")])),
        ])
        .build()
}

/// `cairo_set_line_join` — transcribed from the PHP form.
fn decl_fn_cairo_set_line_join() -> Stmt {
    function("cairo_set_line_join")
        .param("context", t_class("CairoContext"))
        .param("lineJoin", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "setLineJoin", vec![e_var("lineJoin")])),
        ])
        .build()
}

/// `cairo_set_fill_rule` — transcribed from the PHP form.
fn decl_fn_cairo_set_fill_rule() -> Stmt {
    function("cairo_set_fill_rule")
        .param("context", t_class("CairoContext"))
        .param("fillRule", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "setFillRule", vec![e_var("fillRule")])),
        ])
        .build()
}

/// `cairo_move_to` — transcribed from the PHP form.
fn decl_fn_cairo_move_to() -> Stmt {
    function("cairo_move_to")
        .param("context", t_class("CairoContext"))
        .param("x", TypeExpr::Float)
        .param("y", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "moveTo", vec![e_var("x"), e_var("y")])),
        ])
        .build()
}

/// `cairo_line_to` — transcribed from the PHP form.
fn decl_fn_cairo_line_to() -> Stmt {
    function("cairo_line_to")
        .param("context", t_class("CairoContext"))
        .param("x", TypeExpr::Float)
        .param("y", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "lineTo", vec![e_var("x"), e_var("y")])),
        ])
        .build()
}

/// `cairo_curve_to` — transcribed from the PHP form.
fn decl_fn_cairo_curve_to() -> Stmt {
    function("cairo_curve_to")
        .param("context", t_class("CairoContext"))
        .param("x1", TypeExpr::Float)
        .param("y1", TypeExpr::Float)
        .param("x2", TypeExpr::Float)
        .param("y2", TypeExpr::Float)
        .param("x3", TypeExpr::Float)
        .param("y3", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "curveTo", vec![e_var("x1"), e_var("y1"), e_var("x2"), e_var("y2"), e_var("x3"), e_var("y3")])),
        ])
        .build()
}

/// `cairo_rectangle` — transcribed from the PHP form.
fn decl_fn_cairo_rectangle() -> Stmt {
    function("cairo_rectangle")
        .param("context", t_class("CairoContext"))
        .param("x", TypeExpr::Float)
        .param("y", TypeExpr::Float)
        .param("width", TypeExpr::Float)
        .param("height", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "rectangle", vec![e_var("x"), e_var("y"), e_var("width"), e_var("height")])),
        ])
        .build()
}

/// `cairo_arc` — transcribed from the PHP form.
fn decl_fn_cairo_arc() -> Stmt {
    function("cairo_arc")
        .param("context", t_class("CairoContext"))
        .param("xc", TypeExpr::Float)
        .param("yc", TypeExpr::Float)
        .param("radius", TypeExpr::Float)
        .param("angle1", TypeExpr::Float)
        .param("angle2", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "arc", vec![e_var("xc"), e_var("yc"), e_var("radius"), e_var("angle1"), e_var("angle2")])),
        ])
        .build()
}

/// `cairo_arc_negative` — transcribed from the PHP form.
fn decl_fn_cairo_arc_negative() -> Stmt {
    function("cairo_arc_negative")
        .param("context", t_class("CairoContext"))
        .param("xc", TypeExpr::Float)
        .param("yc", TypeExpr::Float)
        .param("radius", TypeExpr::Float)
        .param("angle1", TypeExpr::Float)
        .param("angle2", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "arcNegative", vec![e_var("xc"), e_var("yc"), e_var("radius"), e_var("angle1"), e_var("angle2")])),
        ])
        .build()
}

/// `cairo_close_path` — transcribed from the PHP form.
fn decl_fn_cairo_close_path() -> Stmt {
    function("cairo_close_path")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "closePath", vec![])),
        ])
        .build()
}

/// `cairo_new_path` — transcribed from the PHP form.
fn decl_fn_cairo_new_path() -> Stmt {
    function("cairo_new_path")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "newPath", vec![])),
        ])
        .build()
}

/// `cairo_new_sub_path` — transcribed from the PHP form.
fn decl_fn_cairo_new_sub_path() -> Stmt {
    function("cairo_new_sub_path")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "newSubPath", vec![])),
        ])
        .build()
}

/// `cairo_paint` — transcribed from the PHP form.
fn decl_fn_cairo_paint() -> Stmt {
    function("cairo_paint")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "paint", vec![])),
        ])
        .build()
}

/// `cairo_fill` — transcribed from the PHP form.
fn decl_fn_cairo_fill() -> Stmt {
    function("cairo_fill")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "fill", vec![])),
        ])
        .build()
}

/// `cairo_fill_preserve` — transcribed from the PHP form.
fn decl_fn_cairo_fill_preserve() -> Stmt {
    function("cairo_fill_preserve")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "fillPreserve", vec![])),
        ])
        .build()
}

/// `cairo_stroke` — transcribed from the PHP form.
fn decl_fn_cairo_stroke() -> Stmt {
    function("cairo_stroke")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "stroke", vec![])),
        ])
        .build()
}

/// `cairo_stroke_preserve` — transcribed from the PHP form.
fn decl_fn_cairo_stroke_preserve() -> Stmt {
    function("cairo_stroke_preserve")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "strokePreserve", vec![])),
        ])
        .build()
}

/// `cairo_translate` — transcribed from the PHP form.
fn decl_fn_cairo_translate() -> Stmt {
    function("cairo_translate")
        .param("context", t_class("CairoContext"))
        .param("tx", TypeExpr::Float)
        .param("ty", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "translate", vec![e_var("tx"), e_var("ty")])),
        ])
        .build()
}

/// `cairo_scale` — transcribed from the PHP form.
fn decl_fn_cairo_scale() -> Stmt {
    function("cairo_scale")
        .param("context", t_class("CairoContext"))
        .param("sx", TypeExpr::Float)
        .param("sy", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "scale", vec![e_var("sx"), e_var("sy")])),
        ])
        .build()
}

/// `cairo_rotate` — transcribed from the PHP form.
fn decl_fn_cairo_rotate() -> Stmt {
    function("cairo_rotate")
        .param("context", t_class("CairoContext"))
        .param("angle", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "rotate", vec![e_var("angle")])),
        ])
        .build()
}

/// `cairo_set_matrix` — transcribed from the PHP form.
fn decl_fn_cairo_set_matrix() -> Stmt {
    function("cairo_set_matrix")
        .param("context", t_class("CairoContext"))
        .param("matrix", t_class("CairoMatrix"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "setMatrix", vec![e_var("matrix")])),
        ])
        .build()
}

/// `cairo_transform` — transcribed from the PHP form.
fn decl_fn_cairo_transform() -> Stmt {
    function("cairo_transform")
        .param("context", t_class("CairoContext"))
        .param("matrix", t_class("CairoMatrix"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "transform", vec![e_var("matrix")])),
        ])
        .build()
}

/// `cairo_identity_matrix` — transcribed from the PHP form.
fn decl_fn_cairo_identity_matrix() -> Stmt {
    function("cairo_identity_matrix")
        .param("context", t_class("CairoContext"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("context"), "identityMatrix", vec![])),
        ])
        .build()
}

/// `cairo_get_current_point` — transcribed from the PHP form.
fn decl_fn_cairo_get_current_point() -> Stmt {
    function("cairo_get_current_point")
        .param("context", t_class("CairoContext"))
        .returns(t_array())
        .body(vec![
            s_return(e_array_assoc(vec![(e_str("x"), e_binop(e_call("elephc_cairo_get_current_point_x", vec![e_prop(e_var("context"), "ctx")]), BinOp::Div, e_float(1000.0))), (e_str("y"), e_binop(e_call("elephc_cairo_get_current_point_y", vec![e_prop(e_var("context"), "ctx")]), BinOp::Div, e_float(1000.0)))])),
        ])
        .build()
}

/// `cairo_pattern_create_rgba` — transcribed from the PHP form.
fn decl_fn_cairo_pattern_create_rgba() -> Stmt {
    function("cairo_pattern_create_rgba")
        .param("red", TypeExpr::Float)
        .param("green", TypeExpr::Float)
        .param("blue", TypeExpr::Float)
        .param("alpha", TypeExpr::Float)
        .returns(t_class("CairoSolidPattern"))
        .body(vec![
            s_return(e_new("CairoSolidPattern", vec![e_var("red"), e_var("green"), e_var("blue"), e_var("alpha")])),
        ])
        .build()
}

/// `cairo_pattern_create_rgb` — transcribed from the PHP form.
fn decl_fn_cairo_pattern_create_rgb() -> Stmt {
    function("cairo_pattern_create_rgb")
        .param("red", TypeExpr::Float)
        .param("green", TypeExpr::Float)
        .param("blue", TypeExpr::Float)
        .returns(t_class("CairoSolidPattern"))
        .body(vec![
            s_return(e_static_call("CairoSolidPattern", "createRgb", vec![e_var("red"), e_var("green"), e_var("blue")])),
        ])
        .build()
}

/// `cairo_pattern_create_linear` — transcribed from the PHP form.
fn decl_fn_cairo_pattern_create_linear() -> Stmt {
    function("cairo_pattern_create_linear")
        .param("x0", TypeExpr::Float)
        .param("y0", TypeExpr::Float)
        .param("x1", TypeExpr::Float)
        .param("y1", TypeExpr::Float)
        .returns(t_class("CairoLinearGradient"))
        .body(vec![
            s_return(e_new("CairoLinearGradient", vec![e_var("x0"), e_var("y0"), e_var("x1"), e_var("y1")])),
        ])
        .build()
}

/// `cairo_pattern_create_radial` — transcribed from the PHP form.
fn decl_fn_cairo_pattern_create_radial() -> Stmt {
    function("cairo_pattern_create_radial")
        .param("cx0", TypeExpr::Float)
        .param("cy0", TypeExpr::Float)
        .param("radius0", TypeExpr::Float)
        .param("cx1", TypeExpr::Float)
        .param("cy1", TypeExpr::Float)
        .param("radius1", TypeExpr::Float)
        .returns(t_class("CairoRadialGradient"))
        .body(vec![
            s_return(e_new("CairoRadialGradient", vec![e_var("cx0"), e_var("cy0"), e_var("radius0"), e_var("cx1"), e_var("cy1"), e_var("radius1")])),
        ])
        .build()
}

/// `cairo_pattern_add_color_stop_rgb` — transcribed from the PHP form.
fn decl_fn_cairo_pattern_add_color_stop_rgb() -> Stmt {
    function("cairo_pattern_add_color_stop_rgb")
        .param("pattern", t_class("CairoGradientPattern"))
        .param("offset", TypeExpr::Float)
        .param("red", TypeExpr::Float)
        .param("green", TypeExpr::Float)
        .param("blue", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("pattern"), "addColorStopRgb", vec![e_var("offset"), e_var("red"), e_var("green"), e_var("blue")])),
        ])
        .build()
}

/// `cairo_pattern_add_color_stop_rgba` — transcribed from the PHP form.
fn decl_fn_cairo_pattern_add_color_stop_rgba() -> Stmt {
    function("cairo_pattern_add_color_stop_rgba")
        .param("pattern", t_class("CairoGradientPattern"))
        .param("offset", TypeExpr::Float)
        .param("red", TypeExpr::Float)
        .param("green", TypeExpr::Float)
        .param("blue", TypeExpr::Float)
        .param("alpha", TypeExpr::Float)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_var("pattern"), "addColorStopRgba", vec![e_var("offset"), e_var("red"), e_var("green"), e_var("blue"), e_var("alpha")])),
        ])
        .build()
}

/// `cairo_matrix_init_identity` — transcribed from the PHP form.
fn decl_fn_cairo_matrix_init_identity() -> Stmt {
    function("cairo_matrix_init_identity")
        .returns(t_class("CairoMatrix"))
        .body(vec![
            s_return(e_new("CairoMatrix", vec![])),
        ])
        .build()
}

/// `cairo_matrix_init_translate` — transcribed from the PHP form.
fn decl_fn_cairo_matrix_init_translate() -> Stmt {
    function("cairo_matrix_init_translate")
        .param("tx", TypeExpr::Float)
        .param("ty", TypeExpr::Float)
        .returns(t_class("CairoMatrix"))
        .body(vec![
            s_return(e_new("CairoMatrix", vec![e_float(1.0), e_float(0.0), e_float(0.0), e_float(1.0), e_var("tx"), e_var("ty")])),
        ])
        .build()
}

/// `cairo_matrix_init_scale` — transcribed from the PHP form.
fn decl_fn_cairo_matrix_init_scale() -> Stmt {
    function("cairo_matrix_init_scale")
        .param("sx", TypeExpr::Float)
        .param("sy", TypeExpr::Float)
        .returns(t_class("CairoMatrix"))
        .body(vec![
            s_return(e_new("CairoMatrix", vec![e_var("sx"), e_float(0.0), e_float(0.0), e_var("sy"), e_float(0.0), e_float(0.0)])),
        ])
        .build()
}

/// `cairo_matrix_init_rotate` — transcribed from the PHP form.
fn decl_fn_cairo_matrix_init_rotate() -> Stmt {
    function("cairo_matrix_init_rotate")
        .param("radians", TypeExpr::Float)
        .returns(t_class("CairoMatrix"))
        .body(vec![
            s_assign("m", e_new("CairoMatrix", vec![])),
            s_expr(e_method_call(e_var("m"), "initRotate", vec![e_var("radians")])),
            s_return(e_var("m")),
        ])
        .build()
}

/// `cairo_matrix_multiply` — transcribed from the PHP form.
fn decl_fn_cairo_matrix_multiply() -> Stmt {
    function("cairo_matrix_multiply")
        .param("m1", t_class("CairoMatrix"))
        .param("m2", t_class("CairoMatrix"))
        .returns(t_class("CairoMatrix"))
        .body(vec![
            s_return(e_new("CairoMatrix", vec![e_binop(e_binop(e_prop(e_var("m1"), "xx"), BinOp::Mul, e_prop(e_var("m2"), "xx")), BinOp::Add, e_binop(e_prop(e_var("m1"), "xy"), BinOp::Mul, e_prop(e_var("m2"), "yx"))), e_binop(e_binop(e_prop(e_var("m1"), "yx"), BinOp::Mul, e_prop(e_var("m2"), "xx")), BinOp::Add, e_binop(e_prop(e_var("m1"), "yy"), BinOp::Mul, e_prop(e_var("m2"), "yx"))), e_binop(e_binop(e_prop(e_var("m1"), "xx"), BinOp::Mul, e_prop(e_var("m2"), "xy")), BinOp::Add, e_binop(e_prop(e_var("m1"), "xy"), BinOp::Mul, e_prop(e_var("m2"), "yy"))), e_binop(e_binop(e_prop(e_var("m1"), "yx"), BinOp::Mul, e_prop(e_var("m2"), "xy")), BinOp::Add, e_binop(e_prop(e_var("m1"), "yy"), BinOp::Mul, e_prop(e_var("m2"), "yy"))), e_binop(e_binop(e_binop(e_prop(e_var("m1"), "xx"), BinOp::Mul, e_prop(e_var("m2"), "x0")), BinOp::Add, e_binop(e_prop(e_var("m1"), "xy"), BinOp::Mul, e_prop(e_var("m2"), "y0"))), BinOp::Add, e_prop(e_var("m1"), "x0")), e_binop(e_binop(e_binop(e_prop(e_var("m1"), "yx"), BinOp::Mul, e_prop(e_var("m2"), "x0")), BinOp::Add, e_binop(e_prop(e_var("m1"), "yy"), BinOp::Mul, e_prop(e_var("m2"), "y0"))), BinOp::Add, e_prop(e_var("m1"), "y0"))])),
        ])
        .build()
}

/// `cairo_matrix_transform_point` — transcribed from the PHP form.
fn decl_fn_cairo_matrix_transform_point() -> Stmt {
    function("cairo_matrix_transform_point")
        .param("matrix", t_class("CairoMatrix"))
        .param("x", TypeExpr::Float)
        .param("y", TypeExpr::Float)
        .returns(t_array())
        .body(vec![
            s_return(e_array_assoc(vec![(e_str("x"), e_binop(e_binop(e_binop(e_prop(e_var("matrix"), "xx"), BinOp::Mul, e_var("x")), BinOp::Add, e_binop(e_prop(e_var("matrix"), "xy"), BinOp::Mul, e_var("y"))), BinOp::Add, e_prop(e_var("matrix"), "x0"))), (e_str("y"), e_binop(e_binop(e_binop(e_prop(e_var("matrix"), "yx"), BinOp::Mul, e_var("x")), BinOp::Add, e_binop(e_prop(e_var("matrix"), "yy"), BinOp::Mul, e_var("y"))), BinOp::Add, e_prop(e_var("matrix"), "y0")))])),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn image_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_extern_elephc_img_create_truecolor(),
            decl_extern_elephc_img_create(),
            decl_extern_elephc_img_color_allocate(),
            decl_extern_elephc_img_color_allocate_alpha(),
            decl_extern_elephc_img_set_pixel(),
            decl_extern_elephc_img_sx(),
            decl_extern_elephc_img_sy(),
            decl_extern_elephc_img_is_truecolor(),
            decl_extern_elephc_img_res_x(),
            decl_extern_elephc_img_res_y(),
            decl_extern_elephc_img_set_res(),
            decl_extern_elephc_img_color_at(),
            decl_extern_elephc_img_set_alpha_blending(),
            decl_extern_elephc_img_set_save_alpha(),
            decl_extern_elephc_img_set_transparent(),
            decl_extern_elephc_img_get_transparent(),
            decl_extern_elephc_img_color_total(),
            decl_extern_elephc_img_set_truecolor(),
            decl_extern_elephc_img_set_thickness(),
            decl_extern_elephc_img_line(),
            decl_extern_elephc_img_dashed_line(),
            decl_extern_elephc_img_rectangle(),
            decl_extern_elephc_img_filled_rectangle(),
            decl_extern_elephc_img_ellipse(),
            decl_extern_elephc_img_filled_ellipse(),
            decl_extern_elephc_img_arc(),
            decl_extern_elephc_img_filled_arc(),
            decl_extern_elephc_img_fill(),
            decl_extern_elephc_img_fill_to_border(),
            decl_extern_elephc_img_poly_reset(),
            decl_extern_elephc_img_poly_add(),
            decl_extern_elephc_img_poly_line(),
            decl_extern_elephc_img_poly_fill(),
            decl_extern_elephc_img_string(),
            decl_extern_elephc_img_string_up(),
            decl_extern_elephc_img_destroy(),
            decl_extern_elephc_img_stage_ptr(),
            decl_extern_elephc_img_create_from_stage(),
            decl_extern_elephc_img_create_from_file(),
            decl_extern_elephc_img_write_file(),
            decl_extern_elephc_img_encode(),
            decl_extern_elephc_img_encoded_ptr(),
            decl_extern_elephc_img_encoded_len(),
            decl_extern_elephc_img_encoded_clear(),
            decl_extern_elephc_img_probe_file(),
            decl_extern_elephc_img_probe_stage(),
            decl_extern_elephc_img_probe_width(),
            decl_extern_elephc_img_probe_height(),
            decl_extern_elephc_img_probe_type(),
            decl_extern_elephc_img_probe_bits(),
            decl_extern_elephc_img_probe_channels(),
            decl_extern_elephc_img_fbuf_reset(),
            decl_extern_elephc_img_fbuf_push(),
            decl_extern_elephc_img_copy(),
            decl_extern_elephc_img_copy_merge(),
            decl_extern_elephc_img_copy_merge_gray(),
            decl_extern_elephc_img_copy_resized(),
            decl_extern_elephc_img_copy_resampled(),
            decl_extern_elephc_img_scale(),
            decl_extern_elephc_img_crop(),
            decl_extern_elephc_img_crop_auto(),
            decl_extern_elephc_img_flip(),
            decl_extern_elephc_img_rotate(),
            decl_extern_elephc_img_affine(),
            decl_extern_elephc_img_filter(),
            decl_extern_elephc_img_convolution(),
            decl_extern_elephc_img_gamma(),
            decl_extern_elephc_img_set_interpolation(),
            decl_extern_elephc_img_get_interpolation(),
            decl_extern_elephc_img_set_interlace(),
            decl_extern_elephc_img_get_interlace(),
            decl_extern_elephc_img_in_ptr(),
            decl_extern_elephc_img_out_ptr(),
            decl_extern_elephc_img_kv_count(),
            decl_extern_elephc_img_kv_key(),
            decl_extern_elephc_img_kv_val(),
            decl_extern_elephc_exif_read(),
            decl_extern_elephc_exif_tagname(),
            decl_extern_elephc_exif_thumbnail(),
            decl_extern_elephc_exif_thumb_width(),
            decl_extern_elephc_exif_thumb_height(),
            decl_extern_elephc_exif_thumb_type(),
            decl_extern_elephc_iptc_parse(),
            decl_extern_elephc_iptc_key_count(),
            decl_extern_elephc_iptc_key(),
            decl_extern_elephc_iptc_val_count(),
            decl_extern_elephc_iptc_val(),
            decl_extern_elephc_iptc_embed(),
            decl_extern_elephc_imagick_new(),
            decl_extern_elephc_imagick_destroy(),
            decl_extern_elephc_imagick_clear(),
            decl_extern_elephc_imagick_count(),
            decl_extern_elephc_imagick_read_file(),
            decl_extern_elephc_imagick_read_blob(),
            decl_extern_elephc_imagick_new_image(),
            decl_extern_elephc_imagick_add_image(),
            decl_extern_elephc_imagick_cur_width(),
            decl_extern_elephc_imagick_cur_height(),
            decl_extern_elephc_imagick_set_format(),
            decl_extern_elephc_imagick_get_format(),
            decl_extern_elephc_imagick_set_quality(),
            decl_extern_elephc_imagick_get_quality(),
            decl_extern_elephc_imagick_write_file(),
            decl_extern_elephc_imagick_get_blob(),
            decl_extern_elephc_imagick_get_index(),
            decl_extern_elephc_imagick_set_index(),
            decl_extern_elephc_imagick_next(),
            decl_extern_elephc_imagick_previous(),
            decl_extern_elephc_imagick_first(),
            decl_extern_elephc_imagick_last(),
            decl_extern_elephc_imagick_pixel_color(),
            decl_extern_elephc_imagick_fill(),
            decl_extern_elephc_imagick_resize(),
            decl_extern_elephc_imagick_scale(),
            decl_extern_elephc_imagick_crop(),
            decl_extern_elephc_imagick_rotate(),
            decl_extern_elephc_imagick_flip(),
            decl_extern_elephc_imagick_flop(),
            decl_extern_elephc_imagick_blur(),
            decl_extern_elephc_imagick_negate(),
            decl_extern_elephc_imagick_modulate(),
            decl_extern_elephc_imagick_sharpen(),
            decl_extern_elephc_imagick_composite(),
            decl_extern_elephc_imagick_convolve(),
            decl_extern_elephc_idraw_new(),
            decl_extern_elephc_idraw_destroy(),
            decl_extern_elephc_idraw_clear(),
            decl_extern_elephc_idraw_set_fill(),
            decl_extern_elephc_idraw_set_stroke(),
            decl_extern_elephc_idraw_set_stroke_width(),
            decl_extern_elephc_idraw_get_fill(),
            decl_extern_elephc_idraw_line(),
            decl_extern_elephc_idraw_rectangle(),
            decl_extern_elephc_idraw_circle(),
            decl_extern_elephc_idraw_ellipse(),
            decl_extern_elephc_idraw_point(),
            decl_extern_elephc_idraw_poly_reset(),
            decl_extern_elephc_idraw_poly_point(),
            decl_extern_elephc_idraw_polygon(),
            decl_extern_elephc_imagick_draw(),
            decl_extern_elephc_cairo_surface_create(),
            decl_extern_elephc_cairo_surface_destroy(),
            decl_extern_elephc_cairo_surface_width(),
            decl_extern_elephc_cairo_surface_height(),
            decl_extern_elephc_cairo_surface_encode_png(),
            decl_extern_elephc_cairo_surface_write_png(),
            decl_extern_elephc_cairo_surface_create_from_png(),
            decl_extern_elephc_cairo_create(),
            decl_extern_elephc_cairo_destroy(),
            decl_extern_elephc_cairo_save(),
            decl_extern_elephc_cairo_restore(),
            decl_extern_elephc_cairo_set_source_rgba(),
            decl_extern_elephc_cairo_set_source_pattern(),
            decl_extern_elephc_cairo_set_line_width(),
            decl_extern_elephc_cairo_set_line_cap(),
            decl_extern_elephc_cairo_set_line_join(),
            decl_extern_elephc_cairo_set_fill_rule(),
            decl_extern_elephc_cairo_move_to(),
            decl_extern_elephc_cairo_line_to(),
            decl_extern_elephc_cairo_curve_to(),
            decl_extern_elephc_cairo_rectangle(),
            decl_extern_elephc_cairo_arc(),
            decl_extern_elephc_cairo_arc_negative(),
            decl_extern_elephc_cairo_close_path(),
            decl_extern_elephc_cairo_new_path(),
            decl_extern_elephc_cairo_new_sub_path(),
            decl_extern_elephc_cairo_translate(),
            decl_extern_elephc_cairo_scale(),
            decl_extern_elephc_cairo_rotate(),
            decl_extern_elephc_cairo_set_matrix(),
            decl_extern_elephc_cairo_transform(),
            decl_extern_elephc_cairo_identity_matrix(),
            decl_extern_elephc_cairo_get_current_point_x(),
            decl_extern_elephc_cairo_get_current_point_y(),
            decl_extern_elephc_cairo_paint(),
            decl_extern_elephc_cairo_fill(),
            decl_extern_elephc_cairo_fill_preserve(),
            decl_extern_elephc_cairo_stroke(),
            decl_extern_elephc_cairo_stroke_preserve(),
            decl_extern_elephc_cairo_pattern_create_rgba(),
            decl_extern_elephc_cairo_pattern_create_linear(),
            decl_extern_elephc_cairo_pattern_create_radial(),
            decl_extern_elephc_cairo_pattern_add_color_stop_rgba(),
            decl_extern_elephc_cairo_pattern_destroy(),
            decl_const_imagetype_unknown(),
            decl_const_imagetype_gif(),
            decl_const_imagetype_jpeg(),
            decl_const_imagetype_png(),
            decl_const_imagetype_swf(),
            decl_const_imagetype_psd(),
            decl_const_imagetype_bmp(),
            decl_const_imagetype_tiff_ii(),
            decl_const_imagetype_tiff_mm(),
            decl_const_imagetype_jpc(),
            decl_const_imagetype_jp2(),
            decl_const_imagetype_jpx(),
            decl_const_imagetype_jb2(),
            decl_const_imagetype_swc(),
            decl_const_imagetype_iff(),
            decl_const_imagetype_wbmp(),
            decl_const_imagetype_xbm(),
            decl_const_imagetype_ico(),
            decl_const_imagetype_webp(),
            decl_const_imagetype_avif(),
            decl_const_imagetype_count(),
            decl_const_img_gif(),
            decl_const_img_jpg(),
            decl_const_img_jpeg(),
            decl_const_img_png(),
            decl_const_img_wbmp(),
            decl_const_img_xpm(),
            decl_const_img_webp(),
            decl_const_img_bmp(),
            decl_const_img_tga(),
            decl_const_img_avif(),
            decl_const_img_effect_replace(),
            decl_const_img_effect_alphablend(),
            decl_const_img_effect_normal(),
            decl_const_img_effect_overlay(),
            decl_const_img_effect_multiply(),
            decl_const_img_arc_pie(),
            decl_const_img_arc_chord(),
            decl_const_img_arc_nofill(),
            decl_const_img_arc_edged(),
            decl_const_img_flip_horizontal(),
            decl_const_img_flip_vertical(),
            decl_const_img_flip_both(),
            decl_const_img_filter_negate(),
            decl_const_img_filter_grayscale(),
            decl_const_img_filter_brightness(),
            decl_const_img_filter_contrast(),
            decl_const_img_filter_colorize(),
            decl_const_img_filter_edgedetect(),
            decl_const_img_filter_emboss(),
            decl_const_img_filter_gaussian_blur(),
            decl_const_img_filter_selective_blur(),
            decl_const_img_filter_mean_removal(),
            decl_const_img_filter_smooth(),
            decl_const_img_filter_pixelate(),
            decl_const_img_filter_scatter(),
            decl_const_img_affine_translate(),
            decl_const_img_affine_scale(),
            decl_const_img_affine_rotate(),
            decl_const_img_affine_shear_horizontal(),
            decl_const_img_affine_shear_vertical(),
            decl_const_img_crop_default(),
            decl_const_img_crop_transparent(),
            decl_const_img_crop_black(),
            decl_const_img_crop_white(),
            decl_const_img_crop_sides(),
            decl_const_img_crop_threshold(),
            decl_const_img_bell(),
            decl_const_img_bessel(),
            decl_const_img_bilinear_fixed(),
            decl_const_img_bicubic(),
            decl_const_img_bicubic_fixed(),
            decl_const_img_blackman(),
            decl_const_img_box(),
            decl_const_img_bspline(),
            decl_const_img_catmullrom(),
            decl_const_img_gaussian(),
            decl_const_img_generalized_cubic(),
            decl_const_img_hermite(),
            decl_const_img_hamming(),
            decl_const_img_hanning(),
            decl_const_img_mitchell(),
            decl_const_img_nearest_neighbour(),
            decl_const_img_power(),
            decl_const_img_quadratic(),
            decl_const_img_sinc(),
            decl_const_img_triangle(),
            decl_const_img_weighted4(),
            decl_class_gdimage(),
            decl_fn_imagecreatetruecolor(),
            decl_fn_imagecreate(),
            decl_fn_imagecolorallocate(),
            decl_fn_imagecolorallocatealpha(),
            decl_fn_imagesetpixel(),
            decl_fn_imagesx(),
            decl_fn_imagesy(),
            decl_fn_imagedestroy(),
            decl_fn_imageistruecolor(),
            decl_fn_imageresolution(),
            decl_fn_imagecolorat(),
            decl_fn_imagecolorsforindex(),
            decl_fn_imagecolordeallocate(),
            decl_fn_imagecolorexact(),
            decl_fn_imagecolorexactalpha(),
            decl_fn_imagecolorclosest(),
            decl_fn_imagecolorclosestalpha(),
            decl_fn_imagecolorclosesthwb(),
            decl_fn_imagecolorresolve(),
            decl_fn_imagecolorresolvealpha(),
            decl_fn_imagecolortransparent(),
            decl_fn_imagecolorstotal(),
            decl_fn_imagealphablending(),
            decl_fn_imagesavealpha(),
            decl_fn_imagepalettetotruecolor(),
            decl_fn_imagetruecolortopalette(),
            decl_fn_imagecolormatch(),
            decl_fn_imagecolorset(),
            decl_fn_imagepalettecopy(),
            decl_fn_imagelayereffect(),
            decl_fn_imagesetthickness(),
            decl_fn_imageline(),
            decl_fn_imagedashedline(),
            decl_fn_imagerectangle(),
            decl_fn_imagefilledrectangle(),
            decl_fn_imageellipse(),
            decl_fn_imagefilledellipse(),
            decl_fn_imagearc(),
            decl_fn_imagefilledarc(),
            decl_fn_imagefill(),
            decl_fn_imagefilltoborder(),
            decl_fn_imagepolygon(),
            decl_fn_imageopenpolygon(),
            decl_fn_imagefilledpolygon(),
            decl_fn_imagestring(),
            decl_fn_imagestringup(),
            decl_fn_imagechar(),
            decl_fn_imagecharup(),
            decl_fn_imagefontwidth(),
            decl_fn_imagefontheight(),
            decl_fn_imagecopy(),
            decl_fn_imagecopymerge(),
            decl_fn_imagecopymergegray(),
            decl_fn_imagecopyresized(),
            decl_fn_imagecopyresampled(),
            decl_fn_imagescale(),
            decl_fn_imagecrop(),
            decl_fn_imagecropauto(),
            decl_fn_imageflip(),
            decl_fn_imagerotate(),
            decl_fn_imageaffine(),
            decl_fn_imageaffinematrixconcat(),
            decl_fn_imagefilter(),
            decl_fn_imageconvolution(),
            decl_fn_imagegammacorrect(),
            decl_fn_imagesetinterpolation(),
            decl_fn_imagegetinterpolation(),
            decl_fn_imageinterlace(),
            decl_fn_imageantialias(),
            decl_class_imageexception(),
            decl_fn_imagecreatefrompng(),
            decl_fn_imagecreatefromjpeg(),
            decl_fn_imagecreatefromgif(),
            decl_fn_imagecreatefrombmp(),
            decl_fn_imagecreatefromwebp(),
            decl_fn_imagecreatefromtga(),
            decl_fn_imagecreatefromstring(),
            decl_fn_elephc_img_output(),
            decl_fn_imagepng(),
            decl_fn_imagejpeg(),
            decl_fn_imagegif(),
            decl_fn_imagebmp(),
            decl_fn_imagewebp(),
            decl_fn_imagetypes(),
            decl_fn_gd_info(),
            decl_fn_image_type_to_mime_type(),
            decl_fn_image_type_to_extension(),
            decl_fn_getimagesize(),
            decl_fn_getimagesizefromstring(),
            decl_const_exif_use_mbstring(),
            decl_fn_exif_imagetype(),
            decl_fn_exif_tagname(),
            decl_fn_exif_read_data(),
            decl_fn_read_exif_data(),
            decl_fn_exif_thumbnail(),
            decl_fn_iptcparse(),
            decl_fn_iptcembed(),
            decl_class_imagickexception(),
            decl_class_imagickdrawexception(),
            decl_class_imagickpixelexception(),
            decl_class_imagickpixeliteratorexception(),
            decl_class_imagickkernelexception(),
            decl_fn_imagick_hexval(),
            decl_fn_imagick_color_name(),
            decl_fn_imagick_parse_color(),
            decl_fn_imagick_norm_color(),
            decl_fn_imagick_fmt_to_code(),
            decl_fn_imagick_code_to_fmt(),
            decl_fn_imagick_fmt_from_path(),
            decl_fn_imagick_pack2(),
            decl_fn_imagick_pixel_from_int(),
            decl_class_imagickpixel(),
            decl_class_imagickkernel(),
            decl_class_imagickdraw(),
            decl_class_imagick(),
            decl_class_imagickpixeliterator(),
            decl_class_gmagickexception(),
            decl_class_gmagickdrawexception(),
            decl_class_gmagickpixelexception(),
            decl_fn_gmagick_parse_color(),
            decl_fn_gmagick_norm_color(),
            decl_fn_gmagick_pixel_from_int(),
            decl_class_gmagickpixel(),
            decl_class_gmagickdraw(),
            decl_class_gmagick(),
            decl_class_cairoexception(),
            decl_class_cairoformat(),
            decl_class_cairoantialias(),
            decl_class_cairolinecap(),
            decl_class_cairolinejoin(),
            decl_class_cairofillrule(),
            decl_class_cairofontslant(),
            decl_class_cairofontweight(),
            decl_fn_cairo_fx(),
            decl_fn_cairo_pack(),
            decl_fn_cairo_clamp8(),
            decl_fn_cairo_color(),
            decl_class_cairosurface(),
            decl_class_cairoimagesurface(),
            decl_class_cairopdfsurface(),
            decl_class_cairopssurface(),
            decl_class_cairosvgsurface(),
            decl_class_cairopattern(),
            decl_class_cairosolidpattern(),
            decl_class_cairogradientpattern(),
            decl_class_cairolineargradient(),
            decl_class_cairoradialgradient(),
            decl_class_cairosurfacepattern(),
            decl_class_cairomatrix(),
            decl_class_cairocontext(),
            decl_class_cairofontface(),
            decl_class_cairotoyfontface(),
            decl_class_cairofontoptions(),
            decl_class_cairoscaledfont(),
            decl_class_cairopath(),
            decl_fn_cairo_image_surface_create(),
            decl_fn_cairo_image_surface_create_from_png(),
            decl_fn_cairo_image_surface_get_width(),
            decl_fn_cairo_image_surface_get_height(),
            decl_fn_cairo_surface_write_to_png(),
            decl_fn_cairo_create(),
            decl_fn_cairo_save(),
            decl_fn_cairo_restore(),
            decl_fn_cairo_set_source_rgb(),
            decl_fn_cairo_set_source_rgba(),
            decl_fn_cairo_set_source(),
            decl_fn_cairo_set_line_width(),
            decl_fn_cairo_set_line_cap(),
            decl_fn_cairo_set_line_join(),
            decl_fn_cairo_set_fill_rule(),
            decl_fn_cairo_move_to(),
            decl_fn_cairo_line_to(),
            decl_fn_cairo_curve_to(),
            decl_fn_cairo_rectangle(),
            decl_fn_cairo_arc(),
            decl_fn_cairo_arc_negative(),
            decl_fn_cairo_close_path(),
            decl_fn_cairo_new_path(),
            decl_fn_cairo_new_sub_path(),
            decl_fn_cairo_paint(),
            decl_fn_cairo_fill(),
            decl_fn_cairo_fill_preserve(),
            decl_fn_cairo_stroke(),
            decl_fn_cairo_stroke_preserve(),
            decl_fn_cairo_translate(),
            decl_fn_cairo_scale(),
            decl_fn_cairo_rotate(),
            decl_fn_cairo_set_matrix(),
            decl_fn_cairo_transform(),
            decl_fn_cairo_identity_matrix(),
            decl_fn_cairo_get_current_point(),
            decl_fn_cairo_pattern_create_rgba(),
            decl_fn_cairo_pattern_create_rgb(),
            decl_fn_cairo_pattern_create_linear(),
            decl_fn_cairo_pattern_create_radial(),
            decl_fn_cairo_pattern_add_color_stop_rgb(),
            decl_fn_cairo_pattern_add_color_stop_rgba(),
            decl_fn_cairo_matrix_init_identity(),
            decl_fn_cairo_matrix_init_translate(),
            decl_fn_cairo_matrix_init_scale(),
            decl_fn_cairo_matrix_init_rotate(),
            decl_fn_cairo_matrix_multiply(),
            decl_fn_cairo_matrix_transform_point(),
        ]
    })
}

/// Prepends the image prelude to `program` when it references an image symbol, so
/// the classes, constants, functions, and `elephc_image` externs compile through
/// the normal pipeline only for image-using programs. The prelude carries only
/// declarations (extern block + const + class + functions), which are hoisted, so
/// prepending them ahead of user code does not change top-level execution order.
/// The prelude is static and tested, so a tokenize/parse failure is a compiler
/// bug and panics rather than silently degrading.
///
/// `force` (set by `--with-image`) bypasses the usage scan so the image surface
/// is always injected, making it available even when auto-detection would not see
/// the usage. When every image-shaped call is owned by a user declaration or a
/// canonical guarded polyfill, no bridge surface is injected and the absent-
/// capability branch is activated before name resolution.
pub fn inject_if_used(
    program: crate::parser::ast::Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> crate::parser::ast::Program {
    if !force && !detect::program_uses_image(&program) {
        return detect::activate_guarded_image_polyfills(program);
    }
    let program = detect::deactivate_guarded_image_polyfills(program);
    // Injecting the surface is pay-for-use; what gets injected was not. A program calling two GD
    // functions was dragging Imagick, Gmagick and Cairo through codegen, the assembler and the
    // linker — measured at 162,630 lines of assembly against 9,501.
    //
    // That trimming is NOT done here. A local pass that harvests literal names cannot see a
    // computed one: `$fn = 'image' . 'colorallocate'; $fn($im, …)` had its target removed and the
    // program died with `Call to undefined function`, where PHP answers. The global declaration
    // reachability pass already treats an unknown `$fn()` conservatively, so the COMPLETE selected
    // prelude is recorded and that pass decides what survives.
    let mut combined = image_declarations();
    inventory.record_program("image", &combined);
    combined.extend(program);
    combined
}
