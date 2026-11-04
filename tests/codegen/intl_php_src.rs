//! Purpose:
//! Compiles and runs the committed PHP 8.5.10 `ext/intl/tests` cases.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - The cases live in the generated sibling module `intl_php_src_cases.rs`; each test
//!   carries its PHP source and assertion inline, matching the ordinary Elephc fixture style.
//! - Every fixture gets an isolated temp directory laid out like the php-src test tree so
//!   `__DIR__ . '/../ut_common.inc'` and `_files/` resource paths resolve unchanged.
//! - stdout and stderr share one OS file so warnings and output keep their order.
//! - `--INI--` directives are applied at compile time through the shared override hook and at
//!   run time through startup calls (`error_reporting`, `locale_set_default`,
//!   `date_default_timezone_set`) because the compiled runtime bakes its defaults.
//! - The `.phpt` files under `tests/php_src/intl/` remain the provenance copy and the input for
//!   `scripts/gen_php_src_intl_cases.py`.

use crate::support::*;
use regex::RegexBuilder;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Disambiguates temporary directories across parallel tests.
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

#[path = "intl_php_src_cases.rs"]
mod cases;

/// Restores the compilation context after one fixture's `--INI--` overrides.
struct IniRestore;

impl Drop for IniRestore {
    /// Clears the thread-local overrides so the next fixture starts from defaults.
    fn drop(&mut self) {
        elephc::codegen::set_ini_overrides(Vec::new());
    }
}

/// Converts a php-src `EXPECTF` section into an anchored regular expression.
fn expectf_to_regex(wanted: &str) -> String {
    let wanted = wanted.replace("\r\n", "\n");
    let mut regex = String::new();
    let mut start_offset = 0;
    let bytes = wanted.as_bytes();
    while start_offset < bytes.len() {
        let start = wanted[start_offset..].find("%r").map(|index| start_offset + index);
        let (start, end) = match start {
            Some(start) => match wanted[start + 2..].find("%r").map(|index| start + 2 + index) {
                Some(end) => (start, end),
                None => (bytes.len(), bytes.len()),
            },
            None => (bytes.len(), bytes.len()),
        };
        regex.push_str(&regex::escape(&wanted[start_offset..start]));
        if end > start {
            regex.push('(');
            regex.push_str(&wanted[start + 2..end]);
            regex.push(')');
        }
        start_offset = end + 2;
    }
    for (token, replacement) in [
        ("%e", regex::escape(std::path::MAIN_SEPARATOR_STR)),
        ("%s", r"[^\r\n]+".to_string()),
        ("%S", r"[^\r\n]*".to_string()),
        ("%a", r".+?".to_string()),
        ("%A", r".*?".to_string()),
        ("%w", r"\s*".to_string()),
        ("%i", r"[+-]?\d+".to_string()),
        ("%d", r"\d+".to_string()),
        ("%x", r"[0-9a-fA-F]+".to_string()),
        ("%f", r"[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:[Ee][+-]?\d+)?".to_string()),
        ("%c", ".".to_string()),
        ("%0", r"\x00".to_string()),
    ] {
        regex = regex.replace(&regex::escape(token), &replacement);
    }
    regex
}

/// Returns the committed corpus root.
fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/php_src/intl")
}

/// Copies one directory tree into the fixture work directory.
fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("fixture tree directory");
    for entry in fs::read_dir(source).expect("fixture tree source") {
        let entry = entry.expect("fixture tree entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("fixture tree type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("fixture tree file");
        }
    }
}

/// Adapts php-src's OO/procedural harness to Elephc's supported language surface.
///
/// `ut_common.inc` uses `$GLOBALS['oo-mode']` as a mutable process-wide switch and declares
/// wrappers for `collator_get_default`/`collator_get_variable_top`-style functions that PHP
/// 8.5 no longer ships. Elephc has no `$GLOBALS` superglobal yet, and it resolves every
/// declared function eagerly, so the migration replaces the switch with a static property
/// and removes the unused wrappers. No test calls the removed wrappers.
fn normalize_ut_common(source: &str) -> String {
    let mut text = source.replace("$GLOBALS['oo-mode']", "\\__ElephcIntlMode::$oo");
    // The OO parse methods accept the caller's mixed offset storage and coerce it at the call
    // boundary, so the upstream wrappers keep their `mixed` reference parameters.
    // The wrappers forward the formatter's `int|float|false` result; elephc's mixed-to-union
    // return conversion currently narrows a boxed float to int, so keep the wrapper `mixed`.
    text = text.replace(
        "mixed &$position = null): int|float|false",
        "mixed &$position = null): mixed",
    );
    text = text.replace(
        "function ut_nfmt_format_currency(NumberFormatter $fmt, float $number, string $currency)",
        "function ut_nfmt_format_currency(NumberFormatter $fmt, float $number, mixed $currency)",
    );
    text = text.replace(
        "function ut_nfmt_set_text_attribute(NumberFormatter $fmt, int $attribute, string $value)",
        "function ut_nfmt_set_text_attribute(NumberFormatter $fmt, int $attribute, mixed $value)",
    );
    text = text.replace(
        "function ut_nfmt_set_symbol(NumberFormatter $fmt, int $attribute, string $value)",
        "function ut_nfmt_set_symbol(NumberFormatter $fmt, int $attribute, mixed $value)",
    );
    text = text.replace(
        "function ut_datefmt_parse(IntlDateFormatter $fmt, string $value",
        "function ut_datefmt_parse(IntlDateFormatter $fmt, mixed $value",
    );
    text = text.replace(
        "function ut_datefmt_localtime(IntlDateFormatter $fmt, string $value",
        "function ut_datefmt_localtime(IntlDateFormatter $fmt, mixed $value",
    );
    for function in [
        "ut_coll_get_display_name",
        "ut_coll_get_available_locales",
        "ut_coll_get_variable_top",
        "ut_coll_set_variable_top",
        "ut_coll_restore_variable_top",
        "ut_coll_get_default",
        "ut_coll_set_default",
    ] {
        text = remove_function(&text, function);
    }
    let text = add_mixed_param_types(&text);
    if text.contains("class __ElephcIntlMode") {
        text
    } else {
        text.replacen("<?php", "<?php\nclass __ElephcIntlMode { public static bool $oo = false; }", 1)
    }
}

/// Adds `mixed` to untyped parameters so a harness wrapper is not inferred as `int`.
///
/// Elephc treats an untyped declaration as an inference site, while php-src's wrappers rely on
/// PHP's dynamic `mixed` default. Spelling `mixed` preserves the wrapper's callable surface
/// without changing any call site.
fn add_mixed_param_types(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(relative) = source[cursor..].find("function ") {
        let start = cursor + relative;
        out.push_str(&source[cursor..start]);
        let Some(open) = source[start..].find('(').map(|offset| start + offset) else {
            out.push_str(&source[start..]);
            return out;
        };
        let mut depth = 0usize;
        let mut close = open;
        for (offset, character) in source[open..].char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push_str(&source[start..=open]);
        let params = &source[open + 1..close];
        let mut rendered = Vec::new();
        for param in split_top_level_params(params) {
            let trimmed = param.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(dollar) = trimmed.find('$') {
                let prefix = trimmed[..dollar].trim();
                let prefix_without_markers = prefix.trim_start_matches('&').trim_start_matches("...").trim();
                if prefix_without_markers.is_empty() {
                    rendered.push(format!("mixed {trimmed}"));
                    continue;
                }
            }
            rendered.push(trimmed.to_string());
        }
        out.push_str(&rendered.join(", "));
        out.push(')');
        cursor = close + 1;
    }
    out.push_str(&source[cursor..]);
    out
}

/// Splits a parameter list at top-level commas.
fn split_top_level_params(params: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    for (offset, character) in params.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&params[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
    }
    parts.push(&params[start..]);
    parts
}

/// Removes one top-level `function name(...) { ... }` block when present.
fn remove_function(source: &str, name: &str) -> String {
    let marker = format!("function {name}(");
    let Some(start) = source.find(&marker) else {
        return source.to_string();
    };
    let Some(open) = source[start..].find('{').map(|offset| start + offset) else {
        return source.to_string();
    };
    let mut depth = 0usize;
    let mut end = open;
    for (offset, character) in source[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + offset + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let mut out = String::with_capacity(source.len());
    out.push_str(&source[..start]);
    out.push_str(&source[end..]);
    out
}

/// Copies the root `.inc` files and `_files` tree into every fixture.
fn populate_fixture_tree(work: &Path) {
    let corpus = corpus_root();
    for entry in fs::read_dir(&corpus).expect("corpus root") {
        let entry = entry.expect("corpus entry");
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        if name_text.ends_with(".inc") {
            let text = fs::read_to_string(entry.path()).expect("fixture include");
            let text = if name_text == "ut_common.inc" {
                normalize_ut_common(&text)
            } else {
                text
            };
            fs::write(work.join(&name), text).expect("fixture include");
        }
        if name_text == "_files" {
            copy_tree(&entry.path(), &work.join("_files"));
        }
    }
}

/// Compiles one inline fixture in `work` and returns its merged stdout/stderr.
fn compile_and_capture(source: &str, work: &Path, env: &[(&str, &str)]) -> (bool, String) {
    let (user_asm, runtime_asm, requirements) =
        compile_source_to_asm_with_options(source, work, 8_388_608, false, false);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let object = work.join("fixture.o");
    assemble_from_stdin(&user_asm, &object);
    let binary = work.join("test");
    link_binary(
        &object,
        &runtime_obj,
        &binary,
        &requirements,
        &default_link_paths(),
        &[],
    );
    let log_path = work.join("output.log");
    let log = fs::File::create(&log_path).expect("output log");
    let mut command = Command::new(&binary);
    command.current_dir(work);
    for (key, value) in env {
        command.env(key, value);
    }
    let status = command
        .stdout(Stdio::from(log.try_clone().expect("log handle")))
        .stderr(Stdio::from(log))
        .status()
        .expect("run fixture");
    let output = fs::read_to_string(&log_path).unwrap_or_default();
    (status.success(), output.replace("\r\n", "\n").trim().to_string())
}

/// Applies the runtime-visible part of a fixture's `--INI--` section.
fn apply_runtime_ini(source: &str, ini: &[(&str, &str)]) -> String {
    let mut prefix = String::new();
    for (key, value) in ini {
        match key.to_ascii_lowercase().as_str() {
            "error_reporting" => {
                let compact = value.replace(' ', "");
                let mask = match compact.as_str() {
                    "E_ALL&~E_DEPRECATED" => "E_ALL & ~E_DEPRECATED",
                    "E_ALL" => "E_ALL",
                    "E_ALL&~E_NOTICE" => "E_ALL & ~E_NOTICE",
                    other => other,
                };
                prefix.push_str(&format!("error_reporting({mask});"));
            }
            "intl.default_locale" => {
                prefix.push_str(&format!("locale_set_default({});", php_string(value)));
            }
            // The compiled binary has no `-d` switch, so the fixture's `--INI--` directive
            // becomes the runtime `ini_set()` call the fixture itself would perform.
            "intl.use_exceptions" => {
                let enabled = matches!(value.to_ascii_lowercase().as_str(),
                    "1" | "on" | "true" | "yes");
                prefix.push_str(&format!("ini_set(\"intl.use_exceptions\", {});", enabled));
            }
            "date.timezone" => {
                prefix.push_str(&format!("date_default_timezone_set({});", php_string(value)));
            }
            _ => {}
        }
    }
    if prefix.is_empty() {
        return source.to_string();
    }
    source.replacen("<?php", &format!("<?php {prefix}"), 1)
}

/// Encodes one literal as a PHP single-quoted string.
fn php_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Runs one inline php-src intl fixture and returns its merged output.
pub(super) fn run_case(source: &str, relative_dir: &str, ini: &[(&str, &str)], env: &[(&str, &str)]) -> String {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let work = loop {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos())
            .unwrap_or(0);
        let candidate = std::env::temp_dir().join(format!(
            "elephc_phpt_intl_{}_{:?}_{id}_{nanos}", std::process::id(), std::thread::current().id()
        ));
        match fs::DirBuilder::new().mode(0o700).create(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("temp fixture directory: {error}"),
        }
    };
    populate_fixture_tree(&work);
    // `resourcebundle.inc` probes endianness with `pack()`. Every supported Elephc target is
    // little-endian, and `pack()` is not part of the Intl surface.
    let bundle_inc = work.join("resourcebundle.inc");
    if let Ok(text) = fs::read_to_string(&bundle_inc) {
        if let Some(start) = text.find("if (pack('S', 0xABCD) !== pack('v', 0xABCD)) {") {
            if let Some(relative_end) = text[start..].find("}\n") {
                let end = start + relative_end + 2;
                let mut replaced = String::new();
                replaced.push_str(&text[..start]);
                replaced.push_str("$endian = 'le';\n");
                replaced.push_str(&text[end..]);
                // A constant defined by an included file is not visible inside the
                // includer's functions yet, so BUNDLE is defined by the fixture itself
                // (run_case prepends it) and the include's definition is removed.
                let replaced = replaced.replace(
                    "define('BUNDLE', __DIR__ . '/_files/resourcebundle_' . $endian);",
                    "",
                );
                fs::write(&bundle_inc, replaced).expect("normalize resourcebundle.inc");
            }
        }
    }
    if !source.contains("ut_run(") {
        let common = work.join("ut_common.inc");
        if let Ok(text) = fs::read_to_string(&common) {
            fs::write(&common, remove_function(&text, "ut_run")).expect("normalize ut_common.inc");
        }
    }
    let fixture_dir = if relative_dir.is_empty() { work.clone() } else { work.join(relative_dir) };
    fs::create_dir_all(&fixture_dir).expect("fixture subdirectory");
    let source = apply_runtime_ini(source, ini);
    // Fixtures flip `$GLOBALS['oo-mode']` directly; elephc has no writable superglobals, so the
    // same static-property rewrite the include gets applies to the fixture body as well.
    let source = source.replace("$GLOBALS['oo-mode']", "\\__ElephcIntlMode::$oo");
    // See the BUNDLE note above: define it in the fixture's own compilation unit.
    let source = if source.contains("BUNDLE") {
        source.replacen(
            "<?php",
            "<?php define('BUNDLE', __DIR__ . '/_files/resourcebundle_le');",
            1,
        )
    } else {
        source
    };
    fs::write(fixture_dir.join("test.php"), &source).expect("fixture source");
    let overrides = ini.iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    elephc::codegen::set_ini_overrides(overrides);
    let _restore = IniRestore;

    let (success, actual) = compile_and_capture(&source, &fixture_dir, env);
    let _ = fs::remove_dir_all(&work);
    assert!(success, "compiled fixture exited with failure\n{actual}");
    actual
}

/// Asserts one php-src `EXPECTF` pattern against a fixture's merged output.
pub(super) fn assert_format(actual: &str, expected: &str) {
    let pattern = format!("^{}$", expectf_to_regex(expected.trim()));
    let regex = RegexBuilder::new(&pattern).dot_matches_new_line(true)
        .build()
        .unwrap_or_else(|error| panic!("invalid EXPECTF regex {pattern}: {error}"));
    assert!(regex.is_match(actual), "--- pattern ---\n{pattern}\n--- expected ---\n{}\n--- actual ---\n{actual}",
        expected.trim());
}

/// Asserts one php-src `EXPECTREGEX` pattern against a fixture's merged output.
pub(super) fn assert_regex(actual: &str, expected: &str) {
    let regex = RegexBuilder::new(expected.trim()).dot_matches_new_line(true)
        .build()
        .unwrap_or_else(|error| panic!("invalid EXPECTREGEX pattern: {error}"));
    assert!(regex.is_match(actual), "--- pattern ---\n{}\n--- actual ---\n{actual}", expected.trim());
}










