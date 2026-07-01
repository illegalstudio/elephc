//! Purpose:
//! Groups all `io`-area path and debug builtin homes into this module so the registry
//! can collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its lowering hook (and optional check hook).
//!
//! Called from:
//! - `crate::builtins` (`mod io;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Pure-data builtins (no check hook): var_dump, print_r, basename, realpath_cache_size.
//! - Check-hook builtins: dirname (levels >= 1 constraint), fnmatch (flags type check),
//!   realpath (returns Union(Str, Bool)), realpath_cache_get (returns AssocArray{Str, Mixed}),
//!   pathinfo (flag-dependent return type with static constant folding).
//! - `pathinfo` owns the relocated `pathinfo_static_flag_value` helper (was in io/paths.rs).
//! - Add `pub mod <name>;` here for every new io builtin home.

pub mod basename;
pub mod dirname;
pub mod fnmatch;
pub mod pathinfo;
pub mod print_r;
pub mod realpath;
pub mod realpath_cache_get;
pub mod realpath_cache_size;
pub mod var_dump;
