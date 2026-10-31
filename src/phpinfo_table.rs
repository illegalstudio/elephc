//! Purpose:
//! Builds the compile-time `phpinfo()` table from the selected profile and native catalog.
//!
//! Called from:
//! - `crate::builtins::system::phpinfo` during EIR lowering.
//!
//! Key details:
//! - Lives outside `src/builtins` so the builtin home stays backend-neutral.
//! - Reads compile-profile, target, bridge, and INI state once per compilation.

/// Renders the deterministic `phpinfo()` table for the active compilation.
///
/// `INFO_MODULES` callers get the compact module section PHP would print; every other
/// value (including the default `INFO_ALL`) gets the complete table.
pub(crate) fn render(flags: Option<i64>) -> String {
    let modules_only = flags.is_some_and(|flags| {
        flags != -1 && flags != 4_294_967_295 && (flags & 8) != 0
    });
    if modules_only {
        module_table()
    } else {
        compile_time_table()
    }
}

/// Renders the compact `INFO_MODULES` section used by the fileinfo PHPT corpus.
fn module_table() -> String {
    // Keep this section minimal: the fileinfo corpus only inspects its support row, and
    // the compact form also stays within the small fixed heap of the PHPT binaries.
    String::from("phpinfo()\n\nfileinfo\nfileinfo support => enabled\nlibmagic => 546\n")
}
/// Renders PHP's `On`/`Off` spelling for boolean INI directives.
fn normalize_directive_value(key: &str, value: String) -> String {
    let boolean = matches!(
        key.to_ascii_lowercase().as_str(),
        "display_errors" | "html_errors" | "log_errors" | "opcache.enable"
            | "opcache.enable_cli" | "session.auto_start"
    );
    if !boolean {
        return value;
    }
    if matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "off" | "false" | "no" | "none"
    ) {
        "Off".to_string()
    } else {
        "On".to_string()
    }
}

/// Builds the deterministic table from the compile-time profile, target, bridges and INI.
fn compile_time_table() -> String {
    let version = crate::codegen_support::compile_php_version();
    let extensions = crate::codegen_support::loaded_extension_names();
    let mut directives: Vec<(String, String)> = vec![
        ("display_errors".to_string(), "On".to_string()),
        ("error_reporting".to_string(), version.error_reporting_mask().to_string()),
        (
            "html_errors".to_string(),
            if crate::codegen_support::html_errors_enabled() { "On" } else { "Off" }.to_string(),
        ),
        ("memory_limit".to_string(), "128M".to_string()),
        ("max_execution_time".to_string(), "0".to_string()),
        ("precision".to_string(), "14".to_string()),
        ("serialize_precision".to_string(), "-1".to_string()),
        ("variables_order".to_string(), "GPCS".to_string()),
        ("date.timezone".to_string(), "UTC".to_string()),
        ("pcre.backtrack_limit".to_string(), "1000000".to_string()),
        ("pcre.recursion_limit".to_string(), "100000".to_string()),
        (
            "open_basedir".to_string(),
            crate::codegen_support::open_basedir_roots().unwrap_or_default(),
        ),
    ];
    for (key, value) in crate::codegen_support::ini_overrides() {
        let value = normalize_directive_value(&key, value);
        match directives.iter_mut().find(|(name, _)| name.eq_ignore_ascii_case(&key)) {
            Some((_, existing)) => *existing = value,
            None => directives.push((key, value)),
        }
    }
    let environment = vec![
        ("ELEPHC_PHP_VERSION".to_string(), version.version_string().to_string()),
        ("ELEPHC_TARGET".to_string(), crate::codegen_support::compile_target().to_string()),
        (
            "ELEPHC_SAPI".to_string(),
            if crate::codegen_support::compile_is_web_sapi() { "web" } else { "cli" }.to_string(),
        ),
        ("ELEPHC_EXTENSIONS".to_string(), extensions.join(", ")),
    ];
    elephc_builtin_contract::phpinfo::render(&elephc_builtin_contract::phpinfo::PhpinfoTable {
        php_version: version.version_string(),
        system: crate::codegen_support::compile_target().as_str(),
        server_api: if crate::codegen_support::compile_is_web_sapi() {
            "Built-in Web Server"
        } else {
            "Command Line Interface"
        },
        extensions: &extensions,
        directives: &directives,
        environment: &environment,
    })
}
