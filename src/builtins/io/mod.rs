//! Purpose:
//! Groups all `io`-area path, debug, stat, and filesystem builtin homes into this
//! module so the registry can collect them in one place. Each submodule declares
//! exactly one builtin via `builtin!` and provides its lowering hook (and optional
//! check hook).
//!
//! Called from:
//! - `crate::builtins` (`mod io;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Pure-data builtins (no check hook): var_dump, print_r, basename,
//!   realpath_cache_size, file_exists, is_file, is_dir, is_readable, is_writable,
//!   is_writeable, is_executable, is_link, filesize, filemtime, linkinfo,
//!   disk_free_space, disk_total_space, clearstatcache, getcwd, sys_get_temp_dir,
//!   tempnam, copy, rename, mkdir, rmdir, chdir, symlink, link, umask.
//! - Check-hook builtins: dirname (levels >= 1 constraint), fnmatch (flags type check),
//!   realpath (returns Union(Str, Bool)), realpath_cache_get (returns AssocArray{Str, Mixed}),
//!   pathinfo (flag-dependent return type with static constant folding),
//!   fileatime/filectime/fileperms/fileowner/filegroup/fileinode (Union(Int, Bool)),
//!   filetype (Union(Str, Bool)), stat/lstat/fstat (assoc-array<mixed,int>|bool),
//!   file/scandir/glob (Array<Str>), readfile (Union(Int, Bool)),
//!   readlink (Union(Str, Bool)), chmod (mode must be int),
//!   chown/chgrp/lchown/lchgrp (owner/group must be int or string), touch (timestamp
//!   validation via `check_touch`).
//! - Library-linking check hooks: file_get_contents (TLS / PHAR / z / bz2),
//!   file_put_contents (PHAR / crypto), hash_file (crypto), unlink (PHAR).
//! - Internal PHAR intrinsics (`internal: true`): all 16 `__elephc_phar_*` builtins
//!   migrated from `src/types/checker/builtins/io/files.rs` (io batch C2).
//! - `pathinfo` owns the relocated `pathinfo_static_flag_value` helper (was in io/paths.rs).
//! - `stat_support` holds `stat_result_type` shared by stat/lstat/fstat check hooks.
//! - `touch` owns the relocated `check_touch` helper (was in io/files.rs).
//! - Add `pub mod <name>;` here for every new io builtin home.

pub mod __elephc_phar_bzip2_archive;
pub mod __elephc_phar_decompress_archive;
pub mod __elephc_phar_get_file_metadata;
pub mod __elephc_phar_get_metadata;
pub mod __elephc_phar_get_signature_hash;
pub mod __elephc_phar_get_signature_type;
pub mod __elephc_phar_get_stub;
pub mod __elephc_phar_gzip_archive;
pub mod __elephc_phar_list_entries;
pub mod __elephc_phar_set_compression;
pub mod __elephc_phar_set_file_metadata;
pub mod __elephc_phar_set_metadata;
pub mod __elephc_phar_set_stub;
pub mod __elephc_phar_set_zip_password;
pub mod __elephc_phar_sign_hash;
pub mod __elephc_phar_sign_openssl;
pub mod basename;
pub mod chdir;
pub mod chgrp;
pub mod chmod;
pub mod chown;
pub mod clearstatcache;
pub mod closedir;
pub mod copy;
pub mod dirname;
pub mod disk_free_space;
pub mod disk_total_space;
pub mod fclose;
pub mod fdatasync;
pub mod feof;
pub mod fflush;
pub mod fgetc;
pub mod fgetcsv;
pub mod fgets;
pub mod file;
pub mod file_exists;
pub mod file_get_contents;
pub mod file_put_contents;
pub mod fileatime;
pub mod filectime;
pub mod filegroup;
pub mod fileinode;
pub mod filemtime;
pub mod fileowner;
pub mod fileperms;
pub mod filesize;
pub mod filetype;
pub mod flock;
pub mod fnmatch;
pub mod fopen;
pub mod fpassthru;
pub mod fprintf;
pub mod fputcsv;
pub mod fread;
pub mod fscanf;
pub mod fseek;
pub mod fstat;
pub mod fsync;
pub mod ftell;
pub mod ftruncate;
pub mod fwrite;
pub mod getcwd;
pub mod glob;
pub mod hash_file;
pub mod is_dir;
pub mod is_executable;
pub mod is_file;
pub mod is_link;
pub mod is_readable;
pub mod is_writable;
pub mod is_writeable;
pub mod lchgrp;
pub mod lchown;
pub mod link;
pub mod linkinfo;
pub mod lstat;
pub mod mkdir;
pub mod opendir;
pub mod pathinfo;
pub mod pclose;
pub mod popen;
pub mod print_r;
pub mod readdir;
pub mod readfile;
pub mod readline;
pub mod readlink;
pub mod realpath;
pub mod realpath_cache_get;
pub mod realpath_cache_size;
pub mod rename;
pub mod rewind;
pub mod rewinddir;
pub mod rmdir;
pub mod scandir;
pub mod stat;
pub(crate) mod stat_support;
pub mod symlink;
pub mod sys_get_temp_dir;
pub mod tempnam;
pub mod tmpfile;
pub mod touch;
pub mod umask;
pub mod unlink;
pub mod var_dump;
pub mod vfprintf;
