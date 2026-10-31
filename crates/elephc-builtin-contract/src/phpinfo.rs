//! Purpose:
//! Renders the deterministic compile-time `phpinfo()` table shared by AOT and eval.
//!
//! Called from:
//! - `crate::builtins::system::phpinfo` and Magician's eval `phpinfo` implementation.
//!
//! Key details:
//! - The renderer must stay dependency-neutral: callers pass the selected PHP version,
//!   target, SAPI, linked extensions, directives, and environment rows.
//! - The fileinfo module row is always rendered because the compiler ships that surface.

use std::fmt::Write;

/// Compile-time values one `phpinfo()` table renders.
pub struct PhpinfoTable<'a> {
    /// Reported `PHP_VERSION`.
    pub php_version: &'a str,
    /// Reported `System` value, normally the target triple.
    pub system: &'a str,
    /// Reported `Server API` value.
    pub server_api: &'a str,
    /// Effective loaded extension names.
    pub extensions: &'a [String],
    /// Configuration directives, already merged with compile-time `--ini` overrides.
    pub directives: &'a [(String, String)],
    /// Compile-time environment rows.
    pub environment: &'a [(String, String)],
}

/// Renders a stable plain-text PHP-style information table.
pub fn render(table: &PhpinfoTable<'_>) -> String {
    let mut out = String::new();
    out.push_str("phpinfo()\n\n");
    let _ = writeln!(out, "PHP Version => {}", table.php_version);
    let _ = writeln!(out);
    let _ = writeln!(out, "System => {}", table.system);
    let _ = writeln!(out, "Server API => {}", table.server_api);
    let _ = writeln!(out, "Virtual Directory Support => disabled");
    let _ = writeln!(out, "Configuration File (php.ini) Path => ");
    let _ = writeln!(out, "Loaded Configuration File => (none)");
    let _ = writeln!(out, "Scan this dir for additional .ini files => (none)");
    let _ = writeln!(out, "Additional .ini files parsed => (none)");
    let _ = writeln!(out, "Thread Safety => disabled");
    let _ = writeln!(out, "Zend Signal Handling => enabled");
    let _ = writeln!(out, "Zend Memory Manager => enabled");
    let _ = writeln!(out, "Zend Multibyte Support => provided by mbstring");
    let _ = writeln!(out, "IPv6 Support => enabled");
    let _ = writeln!(out, "DTrace Support => disabled");
    let _ = writeln!(
        out,
        "Registered PHP Streams => https, ftps, compress.zlib, php, file, glob, data, http, ftp, phar, zip"
    );
    let _ = writeln!(
        out,
        "Registered Stream Socket Transports => tcp, udp, unix, udg, ssl, tls, tlsv1.0, tlsv1.1, tlsv1.2, tlsv1.3"
    );
    let _ = writeln!(
        out,
        "Registered Stream Filters => zlib.*, string.rot13, string.toupper, string.tolower, convert.*, consumed, dechunk"
    );
    out.push('\n');

    render_configuration(&mut out, table.directives);
    render_fileinfo(&mut out);
    render_extensions(&mut out, table.extensions);
    render_environment(&mut out, table.environment);
    out
}

/// Renders directives grouped by their dotted prefix, with Core first.
fn render_configuration(out: &mut String, directives: &[(String, String)]) {
    out.push_str("Configuration\n");
    let mut groups: Vec<(String, Vec<&(String, String)>)> = Vec::new();
    for directive in directives {
        let group = directive
            .0
            .split_once('.')
            .map(|(prefix, _)| prefix)
            .unwrap_or("Core");
        match groups.iter_mut().find(|(name, _)| name == group) {
            Some((_, entries)) => entries.push(directive),
            None => groups.push((group.to_string(), vec![directive])),
        }
    }
    groups.sort_by_key(|(name, _)| (name != "Core", name.clone()));
    for (name, entries) in groups {
        let _ = writeln!(out, "{name}");
        for (key, value) in entries {
            let value = if value.is_empty() { "no value" } else { value };
            let _ = writeln!(out, "{key} => {value}");
        }
    }
    out.push('\n');
}

/// Renders the always-present fileinfo module section.
fn render_fileinfo(out: &mut String) {
    out.push_str("fileinfo\n");
    out.push_str("fileinfo support => enabled\n");
    out.push_str("libmagic => 546\n\n");
}

/// Renders the effective loaded-module list.
fn render_extensions(out: &mut String, extensions: &[String]) {
    out.push_str("Loaded Modules\n");
    let mut names: Vec<&str> = extensions.iter().map(String::as_str).collect();
    if !names.iter().any(|name| name.eq_ignore_ascii_case("fileinfo")) {
        names.push("fileinfo");
    }
    let _ = writeln!(out, "{}", names.join(", "));
    out.push('\n');
}

/// Renders the compile-time environment rows.
fn render_environment(out: &mut String, environment: &[(String, String)]) {
    out.push_str("Environment\n");
    for (key, value) in environment {
        let _ = writeln!(out, "{key} => {value}");
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table keeps the fileinfo row, merged directives, and every loaded module.
    #[test]
    fn render_includes_modules_configuration_and_environment() {
        let directives = vec![
            ("display_errors".to_string(), "On".to_string()),
            ("pcre.backtrack_limit".to_string(), "1000000".to_string()),
            ("date.timezone".to_string(), "UTC".to_string()),
        ];
        let extensions = vec!["Core".to_string(), "SPL".to_string(), "fileinfo".to_string()];
        let environment = vec![("ELEPHC_TARGET".to_string(), "linux-x86_64".to_string())];
        let text = render(&PhpinfoTable {
            php_version: "8.5.0",
            system: "linux-x86_64",
            server_api: "Command Line Interface",
            extensions: &extensions,
            directives: &directives,
            environment: &environment,
        });
        assert!(text.contains("PHP Version => 8.5.0"));
        assert!(text.contains("System => linux-x86_64"));
        assert!(text.contains("Core\ndisplay_errors => On"));
        assert!(text.contains("pcre\npcre.backtrack_limit => 1000000"));
        assert!(text.contains("date\ndate.timezone => UTC"));
        assert!(text.contains("fileinfo support => enabled"));
        assert!(text.contains("Loaded Modules\nCore, SPL, fileinfo"));
        assert!(text.contains("Environment\nELEPHC_TARGET => linux-x86_64"));
    }
}
