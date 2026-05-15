//! Purpose:
//! Collects file, directory, path, stat, CSV, glob, and descriptor runtime emitters.
//! The module owns re-export wiring for helpers that adapt PHP I/O builtins to libc and runtime arrays.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` during the I/O runtime section.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

mod basename;
mod cstr;
mod dirname;
mod dirname_levels;
mod feof;
mod fgetcsv;
mod fgets;
mod file;
mod file_get_contents;
mod file_put_contents;
mod fnmatch;
mod fopen;
mod fputcsv;
mod fread;
mod fs;
mod getcwd;
mod glob;
mod modify;
mod modify_x86_64;
mod pathinfo_array;
mod pathinfo_str;
mod realpath;
mod scandir;
mod stat;
mod stat_array;
mod stat_ext;
mod streams_ext;
mod symlink;
mod tempnam;

pub(crate) use basename::emit_basename;
pub(crate) use cstr::emit_cstr;
pub(crate) use dirname::emit_dirname;
pub(crate) use dirname_levels::emit_dirname_levels;
pub(crate) use feof::emit_feof;
pub(crate) use fgetcsv::emit_fgetcsv;
pub(crate) use fgets::emit_fgets;
pub(crate) use file::emit_file;
pub(crate) use file_get_contents::emit_file_get_contents;
pub(crate) use file_put_contents::emit_file_put_contents;
pub(crate) use fnmatch::emit_fnmatch;
pub(crate) use fopen::emit_fopen;
pub(crate) use fputcsv::emit_fputcsv;
pub(crate) use fread::emit_fread;
pub(crate) use fs::emit_fs;
pub(crate) use getcwd::emit_getcwd;
pub(crate) use glob::emit_glob;
pub(crate) use modify::emit_modify;
pub(crate) use pathinfo_array::emit_pathinfo_array;
pub(crate) use pathinfo_str::emit_pathinfo_str;
pub(crate) use realpath::emit_realpath;
pub(crate) use scandir::emit_scandir;
pub(crate) use stat::emit_stat;
pub(crate) use stat_array::emit_stat_array;
pub(crate) use stat_ext::emit_stat_ext;
pub(crate) use streams_ext::emit_streams_ext;
pub(crate) use symlink::emit_symlink;
pub(crate) use tempnam::emit_tempnam;
