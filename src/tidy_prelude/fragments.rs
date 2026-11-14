//! Purpose:
//! The PHP form of the `ext/tidy` prelude: the `tidy` and `tidyNode` classes,
//! the 24 `tidy_*` functions, and the `ob_tidyhandler` output callback, kept as the
//! parse-parity oracle for the built declarations in `crate::tidy_prelude::build` and
//! as the readable reference for the PHP surface.
//!
//! Called from:
//! - `crate::tidy_prelude::oracle_tests` and the transcription driver
//!   (`synthetic_class::transcribe::tests::dump_prelude_on_request`).
//!
//! Key details:
//! - NOT compiled into the prelude: `build.rs` carries the same declarations as AST.
//!   When this text changes, re-transcribe and update the builders, or the oracle fails.
//! - This file is PHP source under `cfg(test)`, not Rust builder code; the compiler
//!   itself never tokenizes it.
//! - The `tidy` object owns one bridge document handle; `tidyNode` objects own
//!   node handles and hold a document reference through the bridge registry.
//! - php-src updates `tidy::$value`/`$errorBuffer` after parse/clean/diagnose;
//!   `__elephc_tidy_sync_properties` reproduces that timing.
//! - `tidyNode` properties are readonly and computed eagerly in the private
//!   constructor, including the recursive `child` array, exactly like php-src's
//!   `tidy_add_node_default_properties`.
//! - Warnings and notices use `__elephc_builtin_warning` / `trigger_error` so the
//!   emitted text keeps PHP's `Warning: <label>: ... in ... on line ...` shape.

/// The complete tidy prelude as PHP source.
pub(super) const SRC: &str = r##"<?php

// -- elephc_tidy bridge --
extern "elephc_tidy" {
    function elephc_tidy_create(): int;
    function elephc_tidy_release(int $doc): int;
    function elephc_tidy_set_encoding(int $doc, string $encoding): int;
    function elephc_tidy_load_config(int $doc, string $path): int;
    function elephc_tidy_parse(int $doc, string $data, int $len): int;
    function elephc_tidy_clean_repair(int $doc): int;
    function elephc_tidy_diagnose(int $doc): int;
    function elephc_tidy_initialized(int $doc): int;
    function elephc_tidy_output(int $doc): string;
    function elephc_tidy_output_present(int $doc): int;
    function elephc_tidy_error_buffer(int $doc): string;
    function elephc_tidy_error_buffer_present(int $doc): int;
    function elephc_tidy_status(int $doc): int;
    function elephc_tidy_html_version(int $doc): int;
    function elephc_tidy_is_xhtml(int $doc): int;
    function elephc_tidy_is_xml(int $doc): int;
    function elephc_tidy_error_count(int $doc): int;
    function elephc_tidy_warning_count(int $doc): int;
    function elephc_tidy_access_count(int $doc): int;
    function elephc_tidy_config_count(int $doc): int;
    function elephc_tidy_release_date(): string;
    function elephc_tidy_option_count(int $doc): int;
    function elephc_tidy_option_index(int $doc, string $name): int;
    function elephc_tidy_option_name(int $doc, int $index): string;
    function elephc_tidy_option_type(int $doc, int $index): int;
    function elephc_tidy_option_internal(int $doc, int $index): int;
    function elephc_tidy_option_value_str(int $doc, int $index): string;
    function elephc_tidy_option_value_int(int $doc, int $index): int;
    function elephc_tidy_option_value_bool(int $doc, int $index): int;
    function elephc_tidy_option_doc(int $doc, int $index): string;
    function elephc_tidy_option_doc_present(int $doc, int $index): int;
    function elephc_tidy_set_option_str(int $doc, string $name, string $value): int;
    function elephc_tidy_set_option_int(int $doc, string $name, int $value): int;
    function elephc_tidy_set_option_bool(int $doc, string $name, int $value): int;
    function elephc_tidy_root(int $doc): int;
    function elephc_tidy_html(int $doc): int;
    function elephc_tidy_head(int $doc): int;
    function elephc_tidy_body(int $doc): int;
    function elephc_tidy_node_release(int $node): int;
    function elephc_tidy_node_type(int $node): int;
    function elephc_tidy_node_name(int $node): string;
    function elephc_tidy_node_text(int $node): string;
    function elephc_tidy_node_line(int $node): int;
    function elephc_tidy_node_column(int $node): int;
    function elephc_tidy_node_proprietary(int $node): int;
    function elephc_tidy_node_id(int $node): int;
    function elephc_tidy_node_has_children(int $node): int;
    function elephc_tidy_node_has_siblings(int $node): int;
    function elephc_tidy_node_is_comment(int $node): int;
    function elephc_tidy_node_is_html(int $node): int;
    function elephc_tidy_node_is_text(int $node): int;
    function elephc_tidy_node_is_jste(int $node): int;
    function elephc_tidy_node_is_asp(int $node): int;
    function elephc_tidy_node_is_php(int $node): int;
    function elephc_tidy_node_parent(int $node): int;
    function elephc_tidy_node_previous(int $node): int;
    function elephc_tidy_node_next(int $node): int;
    function elephc_tidy_node_child_count(int $node): int;
    function elephc_tidy_node_child(int $node, int $index): int;
    function elephc_tidy_node_attr_count(int $node): int;
    function elephc_tidy_node_attr_name(int $node, int $index): string;
    function elephc_tidy_node_attr_value(int $node, int $index): string;
}

/**
 * Emits a warning or notice, or throws the base Exception for the warning forms
 * inside the constructor's `zend_replace_error_handling(EH_THROW, ...)` window.
 * Notices are displayed and never thrown, matching php-src's EH_THROW filter.
 */
function __elephc_tidy_raise(string $message, bool $throw, bool $notice = false): void
{
    // php-src's EH_THROW window converts warnings only; a configuration notice is displayed
    // and the constructor continues. Warnings become base `Exception`, not `ErrorException`.
    if ($notice) {
        trigger_error($message, E_USER_NOTICE);
        return;
    }
    if ($throw) {
        throw new Exception($message);
    }
    __elephc_builtin_warning($message);
}

/**
 * Scans the numeric prefix php-src's `is_numeric_string(..., allow_errors=true)`
 * accepts for an integer/enum configuration value (`"1x"` is the integer 1).
 */
function __elephc_tidy_numeric_prefix(string $text): int|float|false
{
    $_length = strlen($text);
    $_index = 0;
    while ($_index < $_length && strpos(" \t\n\r\v\f", $text[$_index]) !== false) {
        $_index++;
    }
    if ($_index < $_length && ($text[$_index] === '+' || $text[$_index] === '-')) {
        $_index++;
    }
    $_digits = 0;
    while ($_index < $_length && $text[$_index] >= '0' && $text[$_index] <= '9') {
        $_index++;
        $_digits++;
    }
    $_is_float = false;
    if ($_index < $_length && $text[$_index] === '.') {
        $_is_float = true;
        $_index++;
        while ($_index < $_length && $text[$_index] >= '0' && $text[$_index] <= '9') {
            $_index++;
            $_digits++;
        }
    }
    if ($_digits === 0) {
        return false;
    }
    if ($_index < $_length && ($text[$_index] === 'e' || $text[$_index] === 'E')) {
        $_exponent = $_index + 1;
        if ($_exponent < $_length && ($text[$_exponent] === '+' || $text[$_exponent] === '-')) {
            $_exponent++;
        }
        $_exponent_digits = 0;
        while ($_exponent < $_length && $text[$_exponent] >= '0' && $text[$_exponent] <= '9') {
            $_exponent++;
            $_exponent_digits++;
        }
        if ($_exponent_digits > 0) {
            $_is_float = true;
            $_index = $_exponent;
        }
    }
    $_prefix = substr($text, 0, $_index);
    return $_is_float ? (float) $_prefix : (int) $_prefix;
}

/**
 * Applies PHP's weak coercion for the `array|string|null $config` parameter.
 */
function __elephc_tidy_config_argument(mixed $config): array|string|null
{
    if ($config === null || is_array($config) || is_string($config)) {
        return $config;
    }
    return (string) $config;
}

/**
 * Applies PHP's weak coercion for the `?string $encoding` parameter.
 */
function __elephc_tidy_encoding_argument(mixed $encoding): ?string
{
    if ($encoding === null) {
        return null;
    }
    return (string) $encoding;
}

/**
 * Returns the open_basedir violation message for a path, or false when allowed.
 */
function __elephc_tidy_open_basedir_message(string $path): string|false
{
    $_open = __elephc_tidy_ini_open_basedir();
    if ($_open === '') {
        return false;
    }
    $_resolved = realpath($path);
    if ($_resolved === false) {
        if ($path !== '' && $path[0] === '/') {
            $_resolved = $path;
        } else {
            $_cwd = getcwd();
            $_resolved = $_cwd === false ? $path : $_cwd . '/' . $path;
        }
        // php-src resolves the nearest existing parent for a path that does not
        // exist yet. At minimum, normalize lexical `.`/`..` components so a
        // relative path cannot look like it stays under the allowed root.
        $_parts = [];
        foreach (explode('/', $_resolved) as $_part) {
            if ($_part === '' || $_part === '.') {
                continue;
            }
            if ($_part === '..') {
                array_pop($_parts);
                continue;
            }
            $_parts[] = $_part;
        }
        $_resolved = '/' . implode('/', $_parts);
    }
    foreach (explode(':', $_open) as $_root) {
        if ($_root === '') {
            continue;
        }
        $_root_resolved = realpath($_root);
        if ($_root_resolved === false) {
            $_root_resolved = $_root;
        }
        $_prefix = rtrim($_root_resolved, '/') . '/';
        if ($_resolved === $_root_resolved || strpos($_resolved, $_prefix) === 0) {
            return false;
        }
    }
    return 'open_basedir restriction in effect. File(' . $path . ') is not within the allowed path(s): (' . $_open . ')';
}

/**
 * Loads one libtidy configuration file, reproducing php-src's warning/notice
 * split and the constructor's throw mode.
 *
 * `$check_basedir` is false for `tidy.default_config`: php-src's
 * `TIDY_SET_DEFAULT_CONFIG` calls `php_tidy_load_config()` directly, which does
 * not run `php_check_open_basedir()`. User-supplied configuration paths still
 * use the check in `php_tidy_apply_config()`.
 */
function __elephc_tidy_load_config_file(
    int $doc,
    string $path,
    string $label,
    bool $throw,
    bool $check_basedir = true
): bool {
    if ($check_basedir) {
        $_basedir = __elephc_tidy_open_basedir_message($path);
        if ($_basedir !== false) {
            __elephc_tidy_raise($label . ': ' . $_basedir, $throw);
            return false;
        }
    }
    $_status = elephc_tidy_load_config($doc, $path);
    if ($_status < 0) {
        __elephc_tidy_raise($label . ': Could not load the Tidy configuration file "' . $path . '"', $throw);
        return true;
    }
    if ($_status > 0) {
        __elephc_tidy_raise($label . ': There were errors while parsing the Tidy configuration file "' . $path . '"', $throw, true);
    }
    return true;
}

/**
 * Returns the active PHP function label php-src's `php_error_docref(NULL, ...)`
 * would print for a document created by `new tidy`, or `main()` at top level.
 */
function __elephc_tidy_caller_label(): string
{
    foreach (debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 6) as $_frame) {
        $_function = $_frame['function'] ?? '';
        $_class = $_frame['class'] ?? '';
        if ($_function === '' || $_function === '{main}') {
            return 'main()';
        }
        if (strpos($_function, '__elephc_tidy') === 0) {
            continue;
        }
        if ($_class === 'tidy' && ($_function === '__construct' || $_function === '__elephc_wrap')) {
            continue;
        }
        if ($_class !== '') {
            return $_class . '::' . $_function . '()';
        }
        return $_function . '()';
    }
    return 'main()';
}

/**
 * Loads `tidy.default_config` into a freshly created document.
 *
 * `$label` is the php-src `php_error_docref(NULL, ...)` function label. A `null`
 * label means the document was created by `new tidy`, whose label is the active
 * caller of the constructor.
 */
function __elephc_tidy_load_default_config(int $doc, ?string $label = null): void
{
    $_path = __elephc_tidy_ini_default_config();
    if ($_path === '') {
        return;
    }
    if ($label === null) {
        $label = __elephc_tidy_caller_label();
    }
    __elephc_tidy_load_config_file($doc, $_path, (string) $label, false, false);
}

/**
 * Creates a document and applies the default configuration, returning its handle.
 */
function __elephc_tidy_new_doc(string $label): int
{
    $_doc = elephc_tidy_create();
    if ($_doc === 0) {
        throw new Exception('Could not create tidy document');
    }
    __elephc_tidy_load_default_config($_doc, $label);
    return $_doc;
}

/**
 * Applies a PHP string/array/null configuration exactly as php-src does.
 */
function __elephc_tidy_apply_config(int $doc, mixed $config, string $label, bool $throw): bool
{
    $config = __elephc_tidy_config_argument($config);
    if (is_array($config)) {
        if ($config === []) {
            throw new TypeError($label . ': Argument #2 ($config) must be of type array with keys as string');
        }
        $_failures = false;
        foreach ($config as $_key => $_value) {
            if (!is_string($_key)) {
                throw new TypeError($label . ': Argument #2 ($config) must be of type array with keys as string');
            }
            $_index = elephc_tidy_option_index($doc, $_key);
            if ($_index < 0) {
                throw new ValueError($label . ': Argument #2 ($config) Unknown Tidy configuration option "' . $_key . '"');
            }
            if (elephc_tidy_option_internal($doc, $_index) !== 0) {
                throw new ValueError($label . ': Argument #2 ($config) Attempting to set read-only option "' . $_key . '"');
            }
            $_type = elephc_tidy_option_type($doc, $_index);
            if ($_type === 0) {
                $_text = (string) $_value;
                if (elephc_tidy_set_option_str($doc, $_key, $_text) === 0) {
                    throw new TypeError($label . ': Argument #2 ($config) option "' . $_key . '" does not accept "' . $_text . '" as a value');
                }
            } elseif ($_type === 1) {
                if (is_string($_value) || is_object($_value)) {
                    $_text = (string) $_value;
                    $_number = __elephc_tidy_numeric_prefix($_text);
                    if ($_number !== false) {
                        if (elephc_tidy_set_option_int($doc, $_key, (int) $_number) === 0) {
                            $_failures = true;
                        }
                    } elseif (elephc_tidy_set_option_str($doc, $_key, $_text) === 0) {
                        throw new TypeError($label . ': Argument #2 ($config) option "' . $_key . '" does not accept "' . $_text . '" as a value');
                    }
                } else {
                    if (is_float($_value) && (is_nan($_value) || is_infinite($_value))) {
                        if (is_nan($_value)) {
                            $_float_name = 'NAN';
                        } elseif ($_value > 0) {
                            $_float_name = 'INF';
                        } else {
                            $_float_name = '-INF';
                        }
                        __elephc_builtin_warning('The float ' . $_float_name . ' is not representable as an int, cast occurred');
                    }
                    if (elephc_tidy_set_option_int($doc, $_key, (int) $_value) === 0) {
                        $_failures = true;
                    }
                }
            } else {
                if (elephc_tidy_set_option_bool($doc, $_key, (int) $_value) === 0) {
                    $_failures = true;
                }
            }
        }
        return !$_failures;
    }
    if (is_string($config)) {
        return __elephc_tidy_load_config_file($doc, $config, $label, $throw);
    }
    return true;
}

/**
 * Selects the declared encoding, reporting php-src's warning on failure.
 *
 * The parse path treats `null` as "not supplied" but an empty string as a real
 * encoding whose libtidy lookup fails. `tidy_repair_string()`/`tidy_repair_file()`
 * use `Z_PARAM_STRING` in php-src, so an empty string is absent there; callers pass
 * `$skip_empty = true` for that surface.
 */
function __elephc_tidy_set_encoding(
    int $doc,
    mixed $encoding,
    string $label,
    bool $throw,
    bool $skip_empty = false
): bool
{
    $encoding = __elephc_tidy_encoding_argument($encoding);
    if ($encoding === null || ($skip_empty && $encoding === '')) {
        return true;
    }
    if (elephc_tidy_set_encoding($doc, $encoding) < 0) {
        __elephc_tidy_raise($label . ': Could not set encoding "' . $encoding . '"', $throw);
        return false;
    }
    return true;
}

/**
 * Parses a byte string, reporting libtidy's error buffer on failure.
 */
function __elephc_tidy_parse_buffer(int $doc, string $data, string $label, bool $throw): bool
{
    if (strlen($data) > 4294967295) {
        throw new ValueError($label . ': Argument #1 ($string) is too long');
    }
    if (elephc_tidy_parse($doc, $data, strlen($data)) === 0) {
        $_error = elephc_tidy_error_buffer($doc);
        if ($_error !== '') {
            __elephc_tidy_raise($label . ': ' . $_error, $throw);
        }
        return false;
    }
    return true;
}

/**
 * Parses a byte string after applying an optional encoding, as the parse surfaces do.
 */
function __elephc_tidy_parse_into(int $doc, string $data, mixed $encoding, string $label, bool $throw): bool
{
    $encoding = __elephc_tidy_encoding_argument($encoding);
    if (!__elephc_tidy_set_encoding($doc, $encoding, $label, $throw)) {
        return false;
    }
    return __elephc_tidy_parse_buffer($doc, $data, $label, $throw);
}

/**
 * Parses a repair input, matching `php_tidy_quick_repair()`'s continuing error path.
 *
 * php-src records `RETVAL_FALSE` when the encoding is unsupported but still parses and
 * cleans the document; a successful repair overwrites that value with the repaired
 * string. The warning is emitted, but the failure does not abort.
 */
function __elephc_tidy_repair_parse_into(int $doc, string $data, mixed $encoding, string $label): bool
{
    __elephc_tidy_set_encoding($doc, $encoding, $label, false, true);
    return __elephc_tidy_parse_buffer($doc, $data, $label, false);
}

/**
 * Reads a file for parse/repair, reproducing php-src's load diagnostics.
 */
function __elephc_tidy_read_file(string $filename, bool $use_include_path, string $label, string $mode): string|false
{
    if ($filename === '') {
        throw new ValueError('Path must not be empty');
    }
    $_suffix = $use_include_path ? ' (using include path)' : '';
    $_message = 'Cannot load "' . $filename . '" into memory' . $_suffix;
    $_basedir = __elephc_tidy_open_basedir_message($filename);
    if ($_basedir !== false) {
        __elephc_builtin_warning($label . ': ' . $_basedir);
        if ($mode === 'construct') {
            throw new Exception($_message);
        }
        if ($mode !== 'repair') {
            __elephc_builtin_warning($label . ': ' . $_message);
        }
        return false;
    }
    $_contents = @file_get_contents($filename, $use_include_path);
    if ($_contents === false) {
        if ($mode === 'construct') {
            throw new Exception($_message);
        }
        if ($mode === 'repair') {
            return false;
        }
        __elephc_builtin_warning($label . ': ' . $_message);
        return false;
    }
    if (strlen($_contents) > 4294967295) {
        if ($mode === 'repair') {
            throw new ValueError($label . ': Argument #1 ($filename) File content is too long');
        }
        throw new ValueError('File content is too long');
    }
    return $_contents;
}

/**
 * Mirrors php-src's `tidy_doc_update_properties` after parse/clean/diagnose.
 */
function __elephc_tidy_sync_properties(tidy $tidy): void
{
    $_doc = $tidy->__elephc_doc_handle();
    if (elephc_tidy_output_present($_doc) !== 0) {
        $tidy->value = elephc_tidy_output($_doc);
    }
    if (elephc_tidy_error_buffer_present($_doc) !== 0) {
        $tidy->errorBuffer = elephc_tidy_error_buffer($_doc);
    }
}

/**
 * Shared `tidy_getopt`/`tidy::getOpt` implementation with the caller's label.
 */
function __elephc_tidy_get_option(int $doc, string $option, string $label, int $argument): string|int|bool
{
    $_index = elephc_tidy_option_index($doc, $option);
    if ($_index < 0) {
        throw new ValueError($label . ': Argument #' . $argument . ' ($option) is an invalid configuration option, "' . $option . '" given');
    }
    $_type = elephc_tidy_option_type($doc, $_index);
    if ($_type === 0) {
        return elephc_tidy_option_value_str($doc, $_index);
    }
    if ($_type === 1) {
        return elephc_tidy_option_value_int($doc, $_index);
    }
    return elephc_tidy_option_value_bool($doc, $_index) !== 0;
}

/**
 * Shared `tidy_get_opt_doc`/`tidy::getOptDoc` implementation.
 */
function __elephc_tidy_get_option_doc(int $doc, string $option, string $label, int $argument): string|false
{
    $_index = elephc_tidy_option_index($doc, $option);
    if ($_index < 0) {
        throw new ValueError($label . ': Argument #' . $argument . ' ($option) is an invalid configuration option, "' . $option . '" given');
    }
    if (elephc_tidy_option_doc_present($doc, $_index) === 0) {
        return false;
    }
    return elephc_tidy_option_doc($doc, $_index);
}

/**
 * Spells one runtime value's PHP type for a TypeError diagnostic.
 */
function __elephc_tidy_type_name(mixed $value): string
{
    if (is_int($value)) {
        return 'int';
    }
    if (is_float($value)) {
        return 'float';
    }
    if (is_bool($value)) {
        return 'bool';
    }
    if (is_string($value)) {
        return 'string';
    }
    if (is_array($value)) {
        return 'array';
    }
    if (is_null($value)) {
        return 'null';
    }
    if (is_object($value)) {
        return get_class($value);
    }
    if (is_resource($value)) {
        return 'resource';
    }
    return gettype($value);
}

/**
 * Validates a procedural function's `$tidy` argument after the shared catalog
 * accepts `mixed` for class-typed parameters.
 */
function __elephc_tidy_document(mixed $value, string $label): tidy
{
    if (!($value instanceof tidy)) {
        throw new TypeError($label . ': Argument #1 ($tidy) must be of type tidy, ' . __elephc_tidy_type_name($value) . ' given');
    }
    return $value;
}

/**
 * The `tidy` document object.
 */
class tidy
{
    public ?string $errorBuffer = null;
    public ?string $value = null;
    private static array $__elephc_handles = [];
    private static bool $__elephc_minting = false;
    private static int $__elephc_minted_handle = 0;

    public function __construct(?string $filename = null, mixed $config = null, mixed $encoding = null, bool $useIncludePath = false)
    {
        if (self::$__elephc_minting) {
            self::$__elephc_handles[spl_object_id($this)] = self::$__elephc_minted_handle;
            return;
        }
        $_handle = elephc_tidy_create();
        if ($_handle === 0) {
            throw new Exception('Could not create tidy document');
        }
        self::$__elephc_handles[spl_object_id($this)] = $_handle;
        __elephc_tidy_load_default_config($_handle);
        if ($filename !== null) {
            $_contents = __elephc_tidy_read_file($filename, $useIncludePath, 'tidy::__construct()', 'construct');
            if (!__elephc_tidy_apply_config($_handle, $config, 'tidy::__construct()', true)) {
                return;
            }
            if (__elephc_tidy_parse_into($_handle, (string) $_contents, $encoding, 'tidy::__construct()', false)) {
                __elephc_tidy_sync_properties($this);
            }
        }
    }

    public function __destruct()
    {
        $_id = spl_object_id($this);
        if (isset(self::$__elephc_handles[$_id])) {
            $_handle = self::$__elephc_handles[$_id];
            unset(self::$__elephc_handles[$_id]);
            if ($_handle !== 0) {
                elephc_tidy_release($_handle);
            }
        }
    }

    public function __clone()
    {
        throw new Error('Trying to clone an uncloneable object of class ' . get_class($this));
    }

    public function __toString(): string
    {
        return tidy_get_output($this);
    }

    public function __debugInfo(): array
    {
        return [
            'errorBuffer' => $this->errorBuffer,
            'value' => $this->value,
        ];
    }

    public function __elephc_doc_handle(): int
    {
        $_id = spl_object_id($this);
        if (!isset(self::$__elephc_handles[$_id])) {
            // php-src allocates the document in the create_object handler, before the
            // constructor. A subclass that overrides __construct without calling the
            // parent still has a usable document there; allocate lazily for that case.
            $_handle = elephc_tidy_create();
            if ($_handle === 0) {
                throw new Exception('Could not create tidy document');
            }
            self::$__elephc_handles[$_id] = $_handle;
            __elephc_tidy_load_default_config($_handle);
        }
        return self::$__elephc_handles[$_id];
    }

    public static function __elephc_wrap(int $handle): tidy
    {
        self::$__elephc_minted_handle = $handle;
        self::$__elephc_minting = true;
        $_object = new self();
        self::$__elephc_minting = false;
        return $_object;
    }

    public function __elephc_to_int(): int
    {
        return 0;
    }

    public function __elephc_to_float(): float
    {
        return 0.0;
    }

    public function __elephc_to_number(): int
    {
        return 0;
    }

    public function __elephc_to_bool(): bool
    {
        return true;
    }

    public function getOpt(string $option): string|int|bool
    {
        return __elephc_tidy_get_option($this->__elephc_doc_handle(), $option, 'tidy::getOpt()', 1);
    }

    public function cleanRepair(): bool
    {
        $_ok = elephc_tidy_clean_repair($this->__elephc_doc_handle()) !== 0;
        if ($_ok) {
            __elephc_tidy_sync_properties($this);
        }
        return $_ok;
    }

    public function parseFile(string $filename, mixed $config = null, mixed $encoding = null, bool $useIncludePath = false): bool
    {
        $_contents = __elephc_tidy_read_file($filename, $useIncludePath, 'tidy::parseFile()', 'parse');
        if ($_contents === false) {
            return false;
        }
        if (!__elephc_tidy_apply_config($this->__elephc_doc_handle(), $config, 'tidy::parseFile()', false)) {
            return false;
        }
        if (!__elephc_tidy_parse_into($this->__elephc_doc_handle(), (string) $_contents, $encoding, 'tidy::parseFile()', false)) {
            return false;
        }
        __elephc_tidy_sync_properties($this);
        return true;
    }

    public function parseString(string $string, mixed $config = null, mixed $encoding = null): bool
    {
        if (strlen($string) > 4294967295) {
            throw new ValueError('tidy::parseString(): Argument #1 ($string) is too long');
        }
        if (!__elephc_tidy_apply_config($this->__elephc_doc_handle(), $config, 'tidy::parseString()', false)) {
            return false;
        }
        if (!__elephc_tidy_parse_into($this->__elephc_doc_handle(), $string, $encoding, 'tidy::parseString()', false)) {
            return false;
        }
        __elephc_tidy_sync_properties($this);
        return true;
    }

    public static function repairString(string $string, mixed $config = null, mixed $encoding = null): string|false
    {
        return __elephc_tidy_repair_string($string, $config, $encoding, 'tidy::repairString()');
    }

    public static function repairFile(string $filename, mixed $config = null, mixed $encoding = null, bool $useIncludePath = false): string|false
    {
        return __elephc_tidy_repair_file($filename, $config, $encoding, $useIncludePath, 'tidy::repairFile()');
    }

    public function diagnose(): bool
    {
        $_ok = elephc_tidy_diagnose($this->__elephc_doc_handle()) !== 0;
        if ($_ok) {
            __elephc_tidy_sync_properties($this);
        }
        return $_ok;
    }

    public function getRelease(): string
    {
        return elephc_tidy_release_date();
    }

    public function getConfig(): array
    {
        $_config = [];
        $_count = elephc_tidy_option_count($this->__elephc_doc_handle());
        for ($_index = 0; $_index < $_count; $_index++) {
            $_name = elephc_tidy_option_name($this->__elephc_doc_handle(), $_index);
            $_type = elephc_tidy_option_type($this->__elephc_doc_handle(), $_index);
            if ($_type === 0) {
                $_config[$_name] = elephc_tidy_option_value_str($this->__elephc_doc_handle(), $_index);
            } elseif ($_type === 1) {
                $_config[$_name] = elephc_tidy_option_value_int($this->__elephc_doc_handle(), $_index);
            } else {
                $_config[$_name] = elephc_tidy_option_value_bool($this->__elephc_doc_handle(), $_index) !== 0;
            }
        }
        return $_config;
    }

    public function getStatus(): int
    {
        return elephc_tidy_status($this->__elephc_doc_handle());
    }

    public function getHtmlVer(): int
    {
        if (elephc_tidy_initialized($this->__elephc_doc_handle()) === 0) {
            throw new Error('tidy object is not initialized');
        }
        return elephc_tidy_html_version($this->__elephc_doc_handle());
    }

    public function getOptDoc(string $option): string|false
    {
        return __elephc_tidy_get_option_doc($this->__elephc_doc_handle(), $option, 'tidy::getOptDoc()', 1);
    }

    public function isXhtml(): bool
    {
        if (elephc_tidy_initialized($this->__elephc_doc_handle()) === 0) {
            throw new Error('tidy object is not initialized');
        }
        return elephc_tidy_is_xhtml($this->__elephc_doc_handle()) !== 0;
    }

    public function isXml(): bool
    {
        if (elephc_tidy_initialized($this->__elephc_doc_handle()) === 0) {
            throw new Error('tidy object is not initialized');
        }
        return elephc_tidy_is_xml($this->__elephc_doc_handle()) !== 0;
    }

    public function root(): ?tidyNode
    {
        $_handle = elephc_tidy_root($this->__elephc_doc_handle());
        return $_handle === 0 ? null : tidyNode::__elephc_wrap($_handle);
    }

    public function head(): ?tidyNode
    {
        $_handle = elephc_tidy_head($this->__elephc_doc_handle());
        return $_handle === 0 ? null : tidyNode::__elephc_wrap($_handle);
    }

    public function html(): ?tidyNode
    {
        $_handle = elephc_tidy_html($this->__elephc_doc_handle());
        return $_handle === 0 ? null : tidyNode::__elephc_wrap($_handle);
    }

    public function body(): ?tidyNode
    {
        $_handle = elephc_tidy_body($this->__elephc_doc_handle());
        return $_handle === 0 ? null : tidyNode::__elephc_wrap($_handle);
    }
}

/**
 * The `tidyNode` parse-tree node object.
 */
final class tidyNode
{
    public readonly string $value;
    public readonly string $name;
    public readonly int $type;
    public readonly int $line;
    public readonly int $column;
    public readonly bool $proprietary;
    public readonly ?int $id;
    public readonly ?array $attribute;
    public readonly ?array $child;
    private int $__elephc_handle = 0;
    private static bool $__elephc_minting = false;

    private function __construct(int $handle = 0)
    {
        if (!self::$__elephc_minting) {
            $_frames = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 2);
            $_caller = $_frames[1]['class'] ?? '';
            $_scope = $_caller === '' ? 'global scope' : 'scope ' . $_caller;
            throw new Error('Call to private tidyNode::__construct() from ' . $_scope);
        }
        $this->__elephc_handle = $handle;
        $this->value = elephc_tidy_node_text($handle);
        $this->name = elephc_tidy_node_name($handle);
        $this->type = elephc_tidy_node_type($handle);
        $this->line = elephc_tidy_node_line($handle);
        $this->column = elephc_tidy_node_column($handle);
        $this->proprietary = elephc_tidy_node_proprietary($handle) !== 0;
        $_type = $this->type;
        if ($_type === TIDY_NODETYPE_ROOT || $_type === TIDY_NODETYPE_DOCTYPE || $_type === TIDY_NODETYPE_TEXT || $_type === TIDY_NODETYPE_COMMENT) {
            $this->id = null;
        } else {
            $this->id = elephc_tidy_node_id($handle);
        }
        $_attribute_count = elephc_tidy_node_attr_count($handle);
        if ($_attribute_count > 0) {
            $_attributes = [];
            for ($_index = 0; $_index < $_attribute_count; $_index++) {
                $_name = elephc_tidy_node_attr_name($handle, $_index);
                $_attributes[$_name] = elephc_tidy_node_attr_value($handle, $_index);
            }
            $this->attribute = $_attributes;
        } else {
            $this->attribute = null;
        }
        $_child_count = elephc_tidy_node_child_count($handle);
        if ($_child_count > 0) {
            $_children = [];
            for ($_index = 0; $_index < $_child_count; $_index++) {
                $_children[] = self::__elephc_wrap(elephc_tidy_node_child($handle, $_index));
            }
            $this->child = $_children;
        } else {
            $this->child = null;
        }
    }

    public static function __elephc_wrap(int $handle): tidyNode
    {
        self::$__elephc_minting = true;
        try {
            return new self($handle);
        } finally {
            self::$__elephc_minting = false;
        }
    }

    public function __destruct()
    {
        $_handle = $this->__elephc_handle;
        if ($_handle !== 0) {
            $this->__elephc_handle = 0;
            elephc_tidy_node_release($_handle);
        }
    }

    public function __clone()
    {
        throw new Error('Trying to clone an uncloneable object of class ' . get_class($this));
    }

    public function __toString(): string
    {
        return elephc_tidy_node_text($this->__elephc_handle);
    }

    public function __debugInfo(): array
    {
        return [
            'value' => $this->value,
            'name' => $this->name,
            'type' => $this->type,
            'line' => $this->line,
            'column' => $this->column,
            'proprietary' => $this->proprietary,
            'id' => $this->id,
            'attribute' => $this->attribute,
            'child' => $this->child,
        ];
    }

    public function __elephc_to_int(): int
    {
        return 0;
    }

    public function __elephc_to_float(): float
    {
        return 0.0;
    }

    public function __elephc_to_number(): int
    {
        return 0;
    }

    public function __elephc_to_bool(): bool
    {
        return true;
    }

    public function hasChildren(): bool
    {
        return elephc_tidy_node_has_children($this->__elephc_handle) !== 0;
    }

    public function hasSiblings(): bool
    {
        return elephc_tidy_node_has_siblings($this->__elephc_handle) !== 0;
    }

    public function isComment(): bool
    {
        return elephc_tidy_node_is_comment($this->__elephc_handle) !== 0;
    }

    public function isHtml(): bool
    {
        return elephc_tidy_node_is_html($this->__elephc_handle) !== 0;
    }

    public function isText(): bool
    {
        return elephc_tidy_node_is_text($this->__elephc_handle) !== 0;
    }

    public function isJste(): bool
    {
        return elephc_tidy_node_is_jste($this->__elephc_handle) !== 0;
    }

    public function isAsp(): bool
    {
        return elephc_tidy_node_is_asp($this->__elephc_handle) !== 0;
    }

    public function isPhp(): bool
    {
        return elephc_tidy_node_is_php($this->__elephc_handle) !== 0;
    }

    public function getParent(): ?tidyNode
    {
        $_handle = elephc_tidy_node_parent($this->__elephc_handle);
        return $_handle === 0 ? null : self::__elephc_wrap($_handle);
    }

    public function getPreviousSibling(): ?tidyNode
    {
        $_handle = elephc_tidy_node_previous($this->__elephc_handle);
        return $_handle === 0 ? null : self::__elephc_wrap($_handle);
    }

    public function getNextSibling(): ?tidyNode
    {
        $_handle = elephc_tidy_node_next($this->__elephc_handle);
        return $_handle === 0 ? null : self::__elephc_wrap($_handle);
    }
}

function tidy_parse_string(string $string, mixed $config = null, mixed $encoding = null): mixed
{
    if (strlen($string) > 4294967295) {
        throw new ValueError('tidy_parse_string(): Argument #1 ($string) is too long');
    }
    $_doc = __elephc_tidy_new_doc('tidy_parse_string()');
    $_tidy = tidy::__elephc_wrap($_doc);
    if (!__elephc_tidy_apply_config($_doc, $config, 'tidy_parse_string()', false)) {
        return false;
    }
    if (!__elephc_tidy_parse_into($_doc, $string, $encoding, 'tidy_parse_string()', false)) {
        return false;
    }
    __elephc_tidy_sync_properties($_tidy);
    return $_tidy;
}

function tidy_get_error_buffer(mixed $tidy): string|false
{
    $_doc = __elephc_tidy_document($tidy, 'tidy_get_error_buffer()')->__elephc_doc_handle();
    if (elephc_tidy_error_buffer_present($_doc) === 0) {
        return false;
    }
    return elephc_tidy_error_buffer($_doc);
}

function tidy_get_output(mixed $tidy): string
{
    return elephc_tidy_output(__elephc_tidy_document($tidy, 'tidy_get_output()')->__elephc_doc_handle());
}

function tidy_parse_file(string $filename, mixed $config = null, mixed $encoding = null, bool $useIncludePath = false): mixed
{
    $_contents = __elephc_tidy_read_file($filename, $useIncludePath, 'tidy_parse_file()', 'parse');
    if ($_contents === false) {
        return false;
    }
    $_doc = __elephc_tidy_new_doc('tidy_parse_file()');
    $_tidy = tidy::__elephc_wrap($_doc);
    if (!__elephc_tidy_apply_config($_doc, $config, 'tidy_parse_file()', false)) {
        return false;
    }
    if (!__elephc_tidy_parse_into($_doc, (string) $_contents, $encoding, 'tidy_parse_file()', false)) {
        return false;
    }
    __elephc_tidy_sync_properties($_tidy);
    return $_tidy;
}

function tidy_clean_repair(mixed $tidy): bool
{
    return __elephc_tidy_document($tidy, 'tidy_clean_repair()')->cleanRepair();
}

function __elephc_tidy_repair_string(string $string, mixed $config, mixed $encoding, string $label): string|false
{
    if (strlen($string) > 4294967295) {
        throw new ValueError($label . ': Argument #1 ($string) is too long');
    }
    $_doc = __elephc_tidy_new_doc($label);
    try {
        // php_tidy_quick_repair() records a false return for a configuration or
        // encoding failure but keeps going; a successful parse/repair replaces it
        // with the repaired string.
        $_config_ok = __elephc_tidy_apply_config($_doc, $config, $label, false);
        if (!__elephc_tidy_repair_parse_into($_doc, $string, $_config_ok ? $encoding : null, $label)) {
            return false;
        }
        if (elephc_tidy_clean_repair($_doc) === 0) {
            return false;
        }
        return elephc_tidy_output($_doc);
    } finally {
        elephc_tidy_release($_doc);
    }
}

function tidy_repair_string(string $string, mixed $config = null, mixed $encoding = null): string|false
{
    return __elephc_tidy_repair_string($string, $config, $encoding, 'tidy_repair_string()');
}

function __elephc_tidy_repair_file(string $filename, mixed $config, mixed $encoding, bool $useIncludePath, string $label): string|false
{
    $_contents = __elephc_tidy_read_file($filename, $useIncludePath, $label, 'repair');
    if ($_contents === false) {
        return false;
    }
    $_doc = __elephc_tidy_new_doc($label);
    try {
        $_config_ok = __elephc_tidy_apply_config($_doc, $config, $label, false);
        if (!__elephc_tidy_repair_parse_into($_doc, (string) $_contents, $_config_ok ? $encoding : null, $label)) {
            return false;
        }
        if (elephc_tidy_clean_repair($_doc) === 0) {
            return false;
        }
        return elephc_tidy_output($_doc);
    } finally {
        elephc_tidy_release($_doc);
    }
}

function tidy_repair_file(string $filename, mixed $config = null, mixed $encoding = null, bool $useIncludePath = false): string|false
{
    return __elephc_tidy_repair_file($filename, $config, $encoding, $useIncludePath, 'tidy_repair_file()');
}

function tidy_diagnose(mixed $tidy): bool
{
    return __elephc_tidy_document($tidy, 'tidy_diagnose()')->diagnose();
}

function tidy_get_release(): string
{
    return elephc_tidy_release_date();
}

function tidy_get_opt_doc(mixed $tidy, string $option): string|false
{
    return __elephc_tidy_get_option_doc(__elephc_tidy_document($tidy, 'tidy_get_opt_doc()')->__elephc_doc_handle(), $option, 'tidy_get_opt_doc()', 2);
}

function tidy_get_config(mixed $tidy): array
{
    return __elephc_tidy_document($tidy, 'tidy_get_config()')->getConfig();
}

function tidy_get_status(mixed $tidy): int
{
    return elephc_tidy_status(__elephc_tidy_document($tidy, 'tidy_get_status()')->__elephc_doc_handle());
}

function tidy_get_html_ver(mixed $tidy): int
{
    return __elephc_tidy_document($tidy, 'tidy_get_html_ver()')->getHtmlVer();
}

function tidy_is_xhtml(mixed $tidy): bool
{
    return __elephc_tidy_document($tidy, 'tidy_is_xhtml()')->isXhtml();
}

function tidy_is_xml(mixed $tidy): bool
{
    return __elephc_tidy_document($tidy, 'tidy_is_xml()')->isXml();
}

function tidy_error_count(mixed $tidy): int
{
    return elephc_tidy_error_count(__elephc_tidy_document($tidy, 'tidy_error_count()')->__elephc_doc_handle());
}

function tidy_warning_count(mixed $tidy): int
{
    return elephc_tidy_warning_count(__elephc_tidy_document($tidy, 'tidy_warning_count()')->__elephc_doc_handle());
}

function tidy_access_count(mixed $tidy): int
{
    return elephc_tidy_access_count(__elephc_tidy_document($tidy, 'tidy_access_count()')->__elephc_doc_handle());
}

function tidy_config_count(mixed $tidy): int
{
    return elephc_tidy_config_count(__elephc_tidy_document($tidy, 'tidy_config_count()')->__elephc_doc_handle());
}

function tidy_getopt(mixed $tidy, string $option): string|int|bool
{
    return __elephc_tidy_get_option(__elephc_tidy_document($tidy, 'tidy_getopt()')->__elephc_doc_handle(), $option, 'tidy_getopt()', 2);
}

function tidy_get_root(mixed $tidy): ?tidyNode
{
    return __elephc_tidy_document($tidy, 'tidy_get_root()')->root();
}

function tidy_get_html(mixed $tidy): ?tidyNode
{
    return __elephc_tidy_document($tidy, 'tidy_get_html()')->html();
}

function tidy_get_head(mixed $tidy): ?tidyNode
{
    return __elephc_tidy_document($tidy, 'tidy_get_head()')->head();
}

function tidy_get_body(mixed $tidy): ?tidyNode
{
    return __elephc_tidy_document($tidy, 'tidy_get_body()')->body();
}

/**
 * Output-buffering callback registered by `ob_start('ob_tidyhandler')`.
 */
function ob_tidyhandler(string $buffer, int $phase = 0): string
{
    // php-src's output handler processes only the combined START|FINAL window. It
    // cannot repair chunked or intermediate flushes, so those pass through untouched.
    if ($phase < 0
        || ($phase & PHP_OUTPUT_HANDLER_START) === 0
        || ($phase & PHP_OUTPUT_HANDLER_FINAL) === 0) {
        return $buffer;
    }
    $_doc = __elephc_tidy_new_doc('ob_tidyhandler()');
    try {
        // php-src's output handler returns failure silently when libtidy cannot
        // parse or repair the buffer; it does not emit the parse warning the
        // procedural parse surfaces do.
        if (elephc_tidy_parse($_doc, $buffer, strlen($buffer)) === 0) {
            return $buffer;
        }
        if (elephc_tidy_clean_repair($_doc) === 0) {
            return $buffer;
        }
        return elephc_tidy_output($_doc);
    } finally {
        elephc_tidy_release($_doc);
    }
}
"##;
