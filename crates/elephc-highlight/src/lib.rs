//! Purpose:
//! PHP's source highlighter as a native library: the renderer behind `highlight_string()` and
//! `highlight_file()`, linked into compiled programs without the interpreter.
//!
//! Called from:
//! - Compiled programs, through the `elephc_highlight_render` C entry in [`abi`].
//! - `elephc-magician`, whose interpreter builtins render through [`highlight_document`].
//!
//! Key details:
//! - Output matches reference PHP byte for byte (see `render`'s tests).

mod abi;
mod render;

pub use render::{highlight_document, highlight_inner_html};
