//! Purpose:
//! Exercises Core global constants through native code and opaque eval.
//!
//! Called from:
//! - The `codegen_tests` integration test harness.
//!
//! Key details:
//! - Expected values follow the pinned PHP baseline rather than the compiler catalog.

use crate::support::*;

/// Runtime descriptor and eval binders resolve executable-path defaults in the running program.
#[test]
fn test_core_constants_runtime_binary_indirect_defaults() {
    let out = compile_and_run_with_heap_debug(r#"<?php
const PROGRAM_BINARY = PHP_BINARY;
function defaultRuntimePath(string $path = PHP_BINARY): string { return $path; }
function aliasRuntimePath(string $path = PROGRAM_BINARY): string { return $path; }
function arrayRuntimePath(array $paths = [PHP_BINARY]): string { return $paths[0]; }
function mixedRuntimePath(mixed $path = PHP_BINARY): string { return $path; }
function invokeRuntimePath(callable $callback): string { return $callback(); }
class RuntimePathDefaults {
    public function path(string $value = PHP_BINARY): string { return $value; }
}
function invokeMethodRuntimePath(): string {
    $object = new RuntimePathDefaults();
    return invokeRuntimePath($object->path(...));
}
echo invokeRuntimePath(defaultRuntimePath(...)) === PHP_BINARY ? 'callback;' : 'empty;';
echo invokeRuntimePath(aliasRuntimePath(...)) === PHP_BINARY ? 'alias;' : 'empty;';
echo invokeRuntimePath(arrayRuntimePath(...)) === PHP_BINARY ? 'array;' : 'empty;';
echo invokeRuntimePath(mixedRuntimePath(...)) === PHP_BINARY ? 'mixed;' : 'empty;';
echo invokeMethodRuntimePath() === PHP_BINARY ? 'method;' : 'empty;';
$source = 'echo defaultRuntimePath() === PHP_BINARY ? "eval;" : "empty;";
echo arrayRuntimePath() === PHP_BINARY ? "array;" : "empty;";
echo (new RuntimePathDefaults())->path() === PHP_BINARY ? "method;" : "empty;";';
eval($source . ' // ' . $argc);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "callback;alias;array;mixed;method;eval;array;method;", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Executable metadata belongs to the running program and agrees across all lookup routes.
#[test]
fn test_core_constants_runtime_binary() {
    let out = compile_and_run(r#"<?php
const PROGRAM_BINARY = PHP_BINARY;
function executable_path(string $path = PHP_BINARY): string { return $path; }
$binary = PHP_BINARY;
echo basename($binary) === basename($argv[0]) ? 'program:' : 'bad:';
echo file_exists($binary) ? 'exists:' : 'bad:';
echo constant('PHP_BINARY') === $binary && executable_path() === $binary && PROGRAM_BINARY === $binary ? 'direct:' : 'bad:';
$constants = get_defined_constants(true);
echo $constants['Core']['PHP_BINARY'] === $binary && $constants['user']['PROGRAM_BINARY'] === $binary ? 'inventory:' : 'bad:';
$source = '$constants = get_defined_constants(true);
echo PHP_BINARY === $constants["Core"]["PHP_BINARY"] && PROGRAM_BINARY === PHP_BINARY ? "eval:" : "bad:";
return PHP_BINARY;' . ' // ' . $argc;
$evaluated = eval($source);
echo $evaluated === $binary ? 'same' : 'bad';
"#);
    assert_eq!(out, "program:exists:direct:inventory:eval:same");
}

/// Target and build metadata use Elephc's environment rather than baseline-image paths.
#[test]
fn test_core_constants_environment_inventory() {
    let out = compile_and_run(r#"<?php
echo DEFAULT_INCLUDE_PATH, ':', PHP_BUILD_PROVIDER, ':', strlen(PHP_BUILD_DATE), ':';
echo PHP_FD_SETSIZE, ':', PHP_MAXPATHLEN > 0 ? 'path:' : 'bad:';
echo PHP_CONFIG_FILE_PATH === '' && PHP_CONFIG_FILE_SCAN_DIR === '' ? 'no-ini:' : 'bad:';
echo PEAR_INSTALL_DIR === '' && PHP_EXTENSION_DIR === '' ? 'standalone:' : 'bad:';
echo ZEND_VM_KIND, ':';
$source = 'echo PHP_BUILD_PROVIDER, ":", PHP_FD_SETSIZE, ":";
echo PHP_CONFIG_FILE_PATH === "" ? "no-ini:" : "bad:";
echo is_bool(PHP_CLI_PROCESS_TITLE) ? "bool" : "bad";' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, ".:elephc:20:1024:path:no-ini:standalone:ZEND_VM_KIND_NONE:elephc:1024:no-ini:bool");
}

/// Literal constant names retain boolean/null types and PHP's special case-insensitive lookup.
#[test]
fn test_core_constants_literal_names_and_types() {
    let out = compile_and_run(r#"<?php
var_dump(constant('TRUE'), constant('false'), constant('nUlL'));
var_dump(defined('true'), defined('FALSE'), defined('Null'));
var_dump(PHP_DEBUG, PHP_ZTS, ZEND_DEBUG_BUILD, ZEND_THREAD_SAFE);
$constants = get_defined_constants(true);
var_dump($constants['Core']['TRUE'], $constants['Core']['FALSE'], $constants['Core']['NULL']);
$source = 'var_dump(constant("tRuE"), constant("FALSE"), constant("null"));
var_dump(defined("true"), defined("False"), defined("NULL"));
$constants = get_defined_constants(true);
var_dump($constants["Core"]["PHP_DEBUG"], $constants["Core"]["NULL"]);' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, concat!(
        "bool(true)\nbool(false)\nNULL\n",
        "bool(true)\nbool(true)\nbool(true)\n",
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\n",
        "bool(true)\nbool(false)\nNULL\n",
        "bool(true)\nbool(false)\nNULL\n",
        "bool(true)\nbool(true)\nbool(true)\n",
        "bool(false)\nNULL\n",
    ));
}

/// Portable Core flags and upload codes retain their PHP values and module ownership.
#[test]
fn test_core_constants_portable_values_and_inventory() {
    let out = compile_and_run(r#"<?php
echo PHP_FLOAT_DIG, ':', PHP_OUTPUT_HANDLER_STDFLAGS, ':';
echo PHP_OUTPUT_HANDLER_START, ':', PHP_OUTPUT_HANDLER_CLEAN, ':';
echo PHP_OUTPUT_HANDLER_FLUSH, ':', PHP_OUTPUT_HANDLER_FINAL, ':';
echo PHP_OUTPUT_HANDLER_WRITE, ':', PHP_OUTPUT_HANDLER_CONT, ':', PHP_OUTPUT_HANDLER_END, ':';
echo PHP_OUTPUT_HANDLER_CLEANABLE | PHP_OUTPUT_HANDLER_FLUSHABLE | PHP_OUTPUT_HANDLER_REMOVABLE, ':';
echo PHP_OUTPUT_HANDLER_STARTED, ':', PHP_OUTPUT_HANDLER_DISABLED, ':', PHP_OUTPUT_HANDLER_PROCESSED, ':';
echo UPLOAD_ERR_OK, UPLOAD_ERR_INI_SIZE, UPLOAD_ERR_FORM_SIZE, UPLOAD_ERR_PARTIAL;
echo UPLOAD_ERR_NO_FILE, UPLOAD_ERR_NO_TMP_DIR, UPLOAD_ERR_CANT_WRITE, UPLOAD_ERR_EXTENSION, ':';
$constants = get_defined_constants(true);
echo $constants['Core']['PHP_FLOAT_DIG'], ':', $constants['Core']['UPLOAD_ERR_EXTENSION'];
"#);
    assert_eq!(out, "15:112:1:2:4:8:0:0:8:112:4096:8192:16384:01234678:15:8");
}

/// Opaque eval resolves the same flags and reports them in the Core category.
#[test]
fn test_core_constants_portable_values_in_opaque_eval() {
    let out = compile_and_run(r#"<?php
$source = '$constants = get_defined_constants(true);
echo PHP_FLOAT_DIG, ":", PHP_OUTPUT_HANDLER_STDFLAGS, ":", UPLOAD_ERR_EXTENSION;
echo ":", $constants["Core"]["PHP_OUTPUT_HANDLER_FINAL"];
echo ":", defined("UPLOAD_ERR_NO_TMP_DIR") ? "yes" : "bad";
echo ":", defined("upload_err_ok") ? "bad" : "case";' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "15:112:8:8:yes:case");
}
