//! Purpose:
//! Groups the end-to-end codegen tests for the PHP image surface (GD, Exif/IPTC,
//! Imagick, Gmagick, Cairo) on the pure-Rust `elephc_image` bridge.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The image prelude is injected by the compiler/test harness when a program
//!   references an image symbol, and the program links the `elephc-image` bridge
//!   staticlib (a workspace default-member located in `target/<profile>/`). The
//!   `image` crate's codecs are pure Rust, so no external library or system GD is
//!   required and these fixtures are not `#[ignore]`d.

mod foundation;
mod raster_io;
mod color;
mod drawing;
mod text;
mod transform;
mod exif_iptc;
mod imagick;
mod imagick_api_surface;
mod gmagick;
mod gmagick_api_surface;
mod cairo;
mod cairo_procedural;