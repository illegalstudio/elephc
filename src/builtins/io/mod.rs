//! Purpose:
//! Groups all `io`-area path, debug, and stat builtin homes into this module so the
//! registry can collect them in one place. Each submodule declares exactly one builtin
//! via `builtin!` and provides its lowering hook (and optional check hook).
//!
//! Called from:
//! - `crate::builtins` (`mod io;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Pure-data builtins (no check hook): var_dump, print_r, basename, realpath_cache_size,
//!   file_exists, is_file, is_dir, is_readable, is_writable, is_writeable, is_executable,
//!   is_link, filesize, filemtime, linkinfo, disk_free_space, disk_total_space, clearstatcache.
//! - Check-hook builtins: dirname (levels >= 1 constraint), fnmatch (flags type check),
//!   realpath (returns Union(Str, Bool)), realpath_cache_get (returns AssocArray{Str, Mixed}),
//!   pathinfo (flag-dependent return type with static constant folding),
//!   fileatime/filectime/fileperms/fileowner/filegroup/fileinode (Union(Int, Bool)),
//!   filetype (Union(Str, Bool)), stat/lstat/fstat (assoc-array<mixed,int>|bool).
//! - `pathinfo` owns the relocated `pathinfo_static_flag_value` helper (was in io/paths.rs).
//! - `stat_support` holds `stat_result_type` shared by stat/lstat/fstat check hooks.
//! - Add `pub mod <name>;` here for every new io builtin home.

pub mod basename;
pub mod clearstatcache;
pub mod dirname;
pub mod disk_free_space;
pub mod disk_total_space;
pub mod fileatime;
pub mod filectime;
pub mod filegroup;
pub mod fileinode;
pub mod filemtime;
pub mod fileowner;
pub mod fileperms;
pub mod filesize;
pub mod filetype;
pub mod fnmatch;
pub mod fstat;
pub mod is_dir;
pub mod is_executable;
pub mod is_file;
pub mod is_link;
pub mod is_readable;
pub mod is_writable;
pub mod is_writeable;
pub mod linkinfo;
pub mod lstat;
pub mod pathinfo;
pub mod print_r;
pub mod realpath;
pub mod realpath_cache_get;
pub mod realpath_cache_size;
pub mod stat;
pub(crate) mod stat_support;
pub mod file_exists;
pub mod var_dump;
