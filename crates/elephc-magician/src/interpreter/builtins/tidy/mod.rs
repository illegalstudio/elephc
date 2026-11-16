//! Purpose:
//! Forwards eval calls of the `ext/tidy` surface to the host's compiled tidy prelude.
//!
//! Called from:
//! - Eval source-call dispatch, declarative builtin hooks, and dynamic-callable dispatch.
//!
//! Key details:
//! - No native dependency is linked into Magician itself. Every home forwards to the
//!   host function the compiled prelude registered, so eval code runs the same libtidy
//!   bridge as native code.
//! - A host that did not inject the tidy prelude (`--with-tidy`, or a compiled tidy use)
//!   has no registered function, so a call raises PHP's catchable
//!   `Error: Call to undefined function tidy_*()`.
//! - Objects created in eval are real host `tidy` / `tidyNode` objects through the
//!   native-class bridge; no class declarations are re-implemented here.

use super::super::*;

mod dispatch;
mod tidy_parse_string;
mod tidy_get_error_buffer;
mod tidy_get_output;
mod tidy_parse_file;
mod tidy_clean_repair;
mod tidy_repair_string;
mod tidy_repair_file;
mod tidy_diagnose;
mod tidy_get_release;
mod tidy_get_opt_doc;
mod tidy_get_config;
mod tidy_get_status;
mod tidy_get_html_ver;
mod tidy_is_xhtml;
mod tidy_is_xml;
mod tidy_error_count;
mod tidy_warning_count;
mod tidy_access_count;
mod tidy_config_count;
mod tidy_getopt;
mod tidy_get_root;
mod tidy_get_html;
mod tidy_get_head;
mod tidy_get_body;

pub(in crate::interpreter) use dispatch::*;
