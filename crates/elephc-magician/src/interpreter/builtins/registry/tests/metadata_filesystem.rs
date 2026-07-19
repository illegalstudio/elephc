//! Purpose:
//! Registry metadata tests for filesystem path, file, and directory builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Assertions use registry metadata APIs rather than dispatcher literals.

use super::*;

/// Verifies migrated builtin metadata for this registry area.
#[test]
fn declared_builtin_registry_derives_filesystem_metadata() {        assert_eq!(
            eval_declared_builtin_param_names("basename"),
            Some(["path", "suffix"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("basename", 1),
            Some(EvalBuiltinDefaultValue::String(""))
        );
        assert_eq!(
            eval_declared_builtin_default_value("dirname", 1),
            Some(EvalBuiltinDefaultValue::Int(1))
        );
        assert_eq!(
            eval_declared_builtin_default_value("fnmatch", 2),
            Some(EvalBuiltinDefaultValue::Int(0))
        );
        assert_eq!(
            eval_declared_builtin_default_value("pathinfo", 1),
            Some(EvalBuiltinDefaultValue::Int(15))
        );
        assert_eq!(
            eval_declared_builtin_param_names("disk_free_space"),
            Some(["directory"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("getcwd"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("glob"),
            Some(["pattern"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("linkinfo"),
            Some(["path"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("realpath"),
            Some(["path"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stream_resolve_include_path"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("realpath_cache_get"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("sys_get_temp_dir"),
            Some([].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("file_exists"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("file"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("file_get_contents"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("file_put_contents"),
            Some(["filename", "data"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("readfile"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("filemtime"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("filesize"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("is_writable"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("stat"),
            Some(["filename"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("chdir"),
            Some(["directory"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("chmod"),
            Some(["filename", "permissions"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("chown"),
            Some(["filename", "user"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("chgrp"),
            Some(["filename", "group"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_default_value("clearstatcache", 0),
            Some(EvalBuiltinDefaultValue::Bool(false))
        );
        assert_eq!(
            eval_declared_builtin_default_value("clearstatcache", 1),
            Some(EvalBuiltinDefaultValue::String(""))
        );
        assert_eq!(
            eval_declared_builtin_param_names("link"),
            Some(["target", "link"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("rename"),
            Some(["from", "to"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("scandir"),
            Some(["directory"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("tempnam"),
            Some(["directory", "prefix"].as_slice())
        );
        assert_eq!(
            eval_declared_builtin_param_names("popen"),
            Some(["command", "mode"].as_slice())
        );

}
