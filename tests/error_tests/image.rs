//! Purpose:
//! Compile-time diagnostic tests for the image prelude surface (GD, Imagick,
//! Gmagick, Cairo): the type checker rejects wrong argument count and wrong
//! argument type to representative functions/methods once the image prelude is
//! injected.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `check_image` mirrors `check_source` but injects the image prelude
//!   (`image_prelude::inject_if_used`) between alias collection and name
//!   resolution, exactly as the production pipeline does, so the checker sees
//!   the prelude's typed signatures.
//! - `expect_image_error` asserts the program fails to compile, the error names
//!   the flagged callee, and the error is a real type/arity error (it contains
//!   "expects") rather than a missing-prelude "unknown function" error — that
//!   last guard catches a broken injection masquerading as success.

use super::*;

/// Runs the frontend pipeline (tokenize → parse → conditional → autoload
/// aliases → image prelude injection → name resolution → constant folding →
/// type-check) and returns `Ok` if no errors were reported, or `Err(message)`
/// on the first compile error. The image prelude is injected at the same point
/// as in `src/pipeline.rs`.
fn check_image(src: &str) -> Result<(), String> {
    let tokens = tokenize(src).map_err(|e| e.message.clone())?;
    let ast = parse(&tokens).map_err(|e| e.message.clone())?;
    let defines: HashSet<String> = HashSet::new();
    let ast = elephc::conditional::apply(ast, &defines);
    let ast = elephc::autoload::collect_aliases(ast);
    let ast = elephc::image_prelude::inject_if_used(ast, false);
    let ast = elephc::name_resolver::resolve(ast).map_err(|e| e.message.clone())?;
    let ast = elephc::optimize::fold_constants(ast);
    types::check(&ast).map_err(|e| e.message.clone())?;
    Ok(())
}

/// Asserts that `src` fails to compile, that the error names `callee`, and that
/// the error is a type/arity error (contains "expects") rather than a
/// missing-prelude "unknown function" error.
fn expect_image_error(src: &str, callee: &str) {
    let msg = check_image(src)
        .err()
        .unwrap_or_else(|| panic!("Expected error naming '{callee}', but got Ok"));
    assert!(
        msg.contains(callee),
        "Error '{msg}' doesn't name '{callee}'"
    );
    assert!(
        msg.contains("expects"),
        "Error '{msg}' is not a type/arity error (no 'expects'); \
         likely a missing-prelude 'unknown function' error"
    );
}

/// A GD function called with too few arguments is rejected at compile time.
#[test]
fn test_image_error_gd_arity() {
    expect_image_error(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
imagecolorallocate($im);
"#,
        "imagecolorallocate",
    );
}

/// A GD function called with a wrong-typed argument (int where a `GdImage` is
/// expected) is rejected at compile time.
#[test]
fn test_image_error_gd_type() {
    expect_image_error(
        r#"<?php
imagepng(123);
"#,
        "imagepng",
    );
}

/// An Imagick method called with too few arguments is rejected at compile time.
#[test]
fn test_image_error_imagick_arity() {
    expect_image_error(
        r#"<?php
$im = new Imagick();
$im->blurImage();
"#,
        "blurImage",
    );
}

/// An Imagick method called with a wrong-typed argument (string where a `float`
/// is expected) is rejected at compile time. This also guards that
/// `blurImage`'s parameters are typed `float` (the E2 cleanup), so a string
/// radius is flagged rather than silently accepted.
#[test]
fn test_image_error_imagick_type() {
    expect_image_error(
        r#"<?php
$im = new Imagick();
$im->blurImage("x", "y");
"#,
        "blurImage",
    );
}

/// A Gmagick method called with a wrong-typed argument (string where a `float`
/// is expected) is rejected at compile time.
#[test]
fn test_image_error_gmagick_type() {
    expect_image_error(
        r#"<?php
$gm = new Gmagick();
$gm->blurImage("x", 1);
"#,
        "blurImage",
    );
}

/// A Cairo free function called with too few arguments is rejected at compile
/// time. `cairo_image_surface_create` takes a format, width, and height.
#[test]
fn test_image_error_cairo_arity() {
    expect_image_error(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32);
"#,
        "cairo_image_surface_create",
    );
}

/// Asserts that `src` fails to compile specifically because the named Cairo
/// method is *undefined* (an "Undefined method" error). Unlike the dominant
/// Imagick/Gmagick families — where every PHP method is declared as either a
/// real implementation or a throwing stub so the whole surface is callable —
/// PECL cairo is experimental/low-usage and elephc keeps it deliberately
/// partial: the common OOP path is implemented, documented gaps (vector
/// surfaces, text, surface patterns) throw at runtime, and the remaining
/// exotic methods are intentionally left undefined so a call is a compile-time
/// error rather than a silent no-op. This locks that contract.
fn expect_image_undefined_method(src: &str, method: &str) {
    let msg = check_image(src)
        .err()
        .unwrap_or_else(|| panic!("Expected 'Undefined method: {method}', but got Ok"));
    assert!(
        msg.contains(&format!("Undefined method: {method}")),
        "Error '{msg}' does not report '{method}' as undefined"
    );
}

/// Locks the deliberate "undefined Cairo OOP method = compile error" contract
/// for a representative set of common-but-unimplemented `CairoContext`
/// methods (`clip`, `clipPreserve`, `resetClip`, `setDash`, `setMiterLimit`,
/// `hasCurrentPoint`, `inFill`, `copyPath`). Phase C assessment decision: these
/// stay undefined (compile error) rather than being mass-stubbed like the
/// Imagick/Gmagick surface, because PECL cairo is experimental and the common
/// path is already covered.
#[test]
fn test_image_error_cairo_undefined_methods_are_compile_errors() {
    for method in [
        "clip",
        "clipPreserve",
        "resetClip",
        "setDash",
        "setMiterLimit",
        "hasCurrentPoint",
        "inFill",
        "copyPath",
    ] {
        let src = format!(
            r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 4, 4);
$cr = new CairoContext($s);
$cr->{method}();
"#,
        );
        expect_image_undefined_method(&src, &format!("CairoContext::{method}"));
    }
}