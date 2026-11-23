//! Purpose:
//! Lists the predefined PHP constants whose every read raises `E_DEPRECATED`, with the
//! exact php-src message and the first profile that deprecates them.
//!
//! Called from:
//! - The compiler's name resolver (inline run-time deprecation at an ordinary global read),
//!   AST optimizer (a deprecated read is not pure), and EIR lowering (class-constant reads,
//!   reads in constant expressions and initializers, `constant()`).
//! - Magician's constant evaluator (`eval()` reads and `constant()`).
//! - Every caller gates the notice on the program's PHP profile with `applies_to`.
//!
//! Key details:
//! - php-src raises these from the constant fetch itself, so the diagnostic belongs to the
//!   reading expression, and `constant()` reports them too; `defined()` does not.
//! - A class constant is deprecated where it is declared, so the message always names the
//!   declaring class (`DateTimeInterface::RFC7231`) even when it is read through
//!   `DateTime::RFC7231` or a user subclass.
//! - The rows cover every constant PHP 8.5 deprecates that elephc provides.

use crate::PhpVersion;

/// Format string of `DATE_RFC7231` and `DateTimeInterface::RFC7231`.
pub(crate) const RFC7231_FORMAT: &str = "D, d M Y H:i:s \\G\\M\\T";

/// One deprecated constant read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConstantDeprecation {
    /// Full message php-src reports, without the `Deprecated: ` prefix or location.
    pub message: &'static str,
    /// First PHP profile that deprecates the read.
    pub since: PhpVersion,
}

impl ConstantDeprecation {
    /// Reports whether a program compiled for `profile` reports this deprecation.
    pub fn applies_to(self, profile: PhpVersion) -> bool {
        profile >= self.since
    }

    /// Reports whether a profile given as a `PHP_VERSION_ID` reports this deprecation.
    pub fn applies_to_version_id(self, version_id: u32) -> bool {
        version_id >= self.since.version_id()
    }
}

/// One deprecated class constant and the builtin classes that declare it.
struct ClassConstantDeprecation {
    /// Classes whose own declaration is deprecated: the declaring class, plus the builtin
    /// classes elephc gives a copy of the declaration (`DateTime` and `DateTimeImmutable`
    /// carry `DateTimeInterface`'s format constants). Subclasses are found by walking the
    /// class hierarchy to one of these.
    receivers: &'static [&'static str],
    /// Case-sensitive constant name.
    constant: &'static str,
    /// Message and first deprecating profile.
    deprecation: ConstantDeprecation,
}

/// Builds one global table row.
const fn global(
    name: &'static str,
    message: &'static str,
    since: PhpVersion,
) -> (&'static str, ConstantDeprecation) {
    (name, ConstantDeprecation { message, since })
}

/// Builds one class-constant table row.
const fn class(
    receivers: &'static [&'static str],
    constant: &'static str,
    message: &'static str,
    since: PhpVersion,
) -> ClassConstantDeprecation {
    ClassConstantDeprecation { receivers, constant, deprecation: ConstantDeprecation { message, since } }
}

/// Deprecated global constants elephc predefines, keyed by exact (case-sensitive) name, with
/// php-src 8.5's messages verbatim. Sorted by name.
static GLOBAL_CONSTANT_DEPRECATIONS: &[(&str, ConstantDeprecation)] = &[
    global("CURLOPT_BINARYTRANSFER", "Constant CURLOPT_BINARYTRANSFER is deprecated since 8.4, as it had no effect since 5.1.2", PhpVersion::Php84),
    global("DATE_RFC7231", "Constant DATE_RFC7231 is deprecated since 8.5, as this format ignores the associated timezone and always uses GMT", PhpVersion::Php85),
    global("E_STRICT", "Constant E_STRICT is deprecated since 8.4, the error level was removed", PhpVersion::Php84),
    global("FILTER_SANITIZE_STRING", "Constant FILTER_SANITIZE_STRING is deprecated since 8.1, use htmlspecialchars() instead", PhpVersion::Php81),
    global("FILTER_SANITIZE_STRIPPED", "Constant FILTER_SANITIZE_STRIPPED is deprecated since 8.1, use htmlspecialchars() instead", PhpVersion::Php81),
    global("MT_RAND_PHP", "Constant MT_RAND_PHP is deprecated since 8.3, as it uses a biased non-standard variant of Mt19937", PhpVersion::Php83),
    global("MYSQLI_DATA_TRUNCATED", "Constant MYSQLI_DATA_TRUNCATED is deprecated since 8.1, as it was unused", PhpVersion::Php81),
    global("MYSQLI_IS_MARIADB", "Constant MYSQLI_IS_MARIADB is deprecated since 8.2, as it is always false", PhpVersion::Php82),
    global("MYSQLI_NO_DATA", "Constant MYSQLI_NO_DATA is deprecated since 8.1, as it was unused", PhpVersion::Php81),
    global("MYSQLI_REFRESH_BACKUP_LOG", "Constant MYSQLI_REFRESH_BACKUP_LOG is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_REFRESH_GRANT", "Constant MYSQLI_REFRESH_GRANT is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_REFRESH_HOSTS", "Constant MYSQLI_REFRESH_HOSTS is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_REFRESH_LOG", "Constant MYSQLI_REFRESH_LOG is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_REFRESH_MASTER", "Constant MYSQLI_REFRESH_MASTER is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_REFRESH_REPLICA", "Constant MYSQLI_REFRESH_REPLICA is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_REFRESH_SLAVE", "Constant MYSQLI_REFRESH_SLAVE is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_REFRESH_STATUS", "Constant MYSQLI_REFRESH_STATUS is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_REFRESH_TABLES", "Constant MYSQLI_REFRESH_TABLES is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_REFRESH_THREADS", "Constant MYSQLI_REFRESH_THREADS is deprecated since 8.4, as mysqli_refresh() is deprecated", PhpVersion::Php84),
    global("MYSQLI_SERVER_PS_OUT_PARAMS", "Constant MYSQLI_SERVER_PS_OUT_PARAMS is deprecated since 8.1, as it was unused", PhpVersion::Php81),
    global("MYSQLI_SERVER_QUERY_NO_GOOD_INDEX_USED", "Constant MYSQLI_SERVER_QUERY_NO_GOOD_INDEX_USED is deprecated since 8.1, as it was unused", PhpVersion::Php81),
    global("MYSQLI_SERVER_QUERY_NO_INDEX_USED", "Constant MYSQLI_SERVER_QUERY_NO_INDEX_USED is deprecated since 8.1, as it was unused", PhpVersion::Php81),
    global("MYSQLI_SERVER_QUERY_WAS_SLOW", "Constant MYSQLI_SERVER_QUERY_WAS_SLOW is deprecated since 8.1, as it was unused", PhpVersion::Php81),
    global("MYSQLI_STORE_RESULT_COPY_DATA", "Constant MYSQLI_STORE_RESULT_COPY_DATA is deprecated since 8.4, as the mysqli_store_result() parameter is unused since 8.1", PhpVersion::Php84),
    global("SUNFUNCS_RET_DOUBLE", "Constant SUNFUNCS_RET_DOUBLE is deprecated since 8.4, as date_sunrise() and date_sunset() were deprecated in 8.1", PhpVersion::Php84),
    global("SUNFUNCS_RET_STRING", "Constant SUNFUNCS_RET_STRING is deprecated since 8.4, as date_sunrise() and date_sunset() were deprecated in 8.1", PhpVersion::Php84),
    global("SUNFUNCS_RET_TIMESTAMP", "Constant SUNFUNCS_RET_TIMESTAMP is deprecated since 8.4, as date_sunrise() and date_sunset() were deprecated in 8.1", PhpVersion::Php84),
];

/// Deprecated constants of builtin classes elephc provides, with php-src 8.5's messages
/// verbatim. `PDO::MYSQL_ATTR_*` and `PDO::PGSQL_*` are not listed because elephc does not
/// declare them (reading one is a compile error); `NumberFormatter` and `ZipArchive` are not
/// provided at all.
static CLASS_CONSTANT_DEPRECATIONS: &[ClassConstantDeprecation] = &[
    class(&["DateTimeInterface", "DateTime", "DateTimeImmutable"], "RFC7231", "Constant DateTimeInterface::RFC7231 is deprecated since 8.5, as this format ignores the associated timezone and always uses GMT", PhpVersion::Php85),
    class(&["PDO"], "SQLITE_ATTR_EXTENDED_RESULT_CODES", "Constant PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES is deprecated since 8.5, use Pdo\\Sqlite::ATTR_EXTENDED_RESULT_CODES instead", PhpVersion::Php85),
    class(&["PDO"], "SQLITE_ATTR_OPEN_FLAGS", "Constant PDO::SQLITE_ATTR_OPEN_FLAGS is deprecated since 8.5, use Pdo\\Sqlite::ATTR_OPEN_FLAGS instead", PhpVersion::Php85),
    class(&["PDO"], "SQLITE_ATTR_READONLY_STATEMENT", "Constant PDO::SQLITE_ATTR_READONLY_STATEMENT is deprecated since 8.5, use Pdo\\Sqlite::ATTR_READONLY_STATEMENT instead", PhpVersion::Php85),
    class(&["PDO"], "SQLITE_DETERMINISTIC", "Constant PDO::SQLITE_DETERMINISTIC is deprecated since 8.5, use Pdo\\Sqlite::DETERMINISTIC instead", PhpVersion::Php85),
    class(&["PDO"], "SQLITE_OPEN_CREATE", "Constant PDO::SQLITE_OPEN_CREATE is deprecated since 8.5, use Pdo\\Sqlite::OPEN_CREATE instead", PhpVersion::Php85),
    class(&["PDO"], "SQLITE_OPEN_READONLY", "Constant PDO::SQLITE_OPEN_READONLY is deprecated since 8.5, use Pdo\\Sqlite::OPEN_READONLY instead", PhpVersion::Php85),
    class(&["PDO"], "SQLITE_OPEN_READWRITE", "Constant PDO::SQLITE_OPEN_READWRITE is deprecated since 8.5, use Pdo\\Sqlite::OPEN_READWRITE instead", PhpVersion::Php85),
    class(&["Pdo\\Pgsql"], "TRANSACTION_ACTIVE", "Constant Pdo\\Pgsql::TRANSACTION_ACTIVE is deprecated since 8.5, as it has no effect", PhpVersion::Php85),
    class(&["Pdo\\Pgsql"], "TRANSACTION_IDLE", "Constant Pdo\\Pgsql::TRANSACTION_IDLE is deprecated since 8.5, as it has no effect", PhpVersion::Php85),
    class(&["Pdo\\Pgsql"], "TRANSACTION_INERROR", "Constant Pdo\\Pgsql::TRANSACTION_INERROR is deprecated since 8.5, as it has no effect", PhpVersion::Php85),
    class(&["Pdo\\Pgsql"], "TRANSACTION_INTRANS", "Constant Pdo\\Pgsql::TRANSACTION_INTRANS is deprecated since 8.5, as it has no effect", PhpVersion::Php85),
    class(&["Pdo\\Pgsql"], "TRANSACTION_UNKNOWN", "Constant Pdo\\Pgsql::TRANSACTION_UNKNOWN is deprecated since 8.5, as it has no effect", PhpVersion::Php85),
];

/// Returns the deprecation PHP raises when a program reads the global constant `name`
/// (exact case, optional leading `\`), regardless of profile.
pub fn global_constant_deprecation(name: &str) -> Option<ConstantDeprecation> {
    let name = name.strip_prefix('\\').unwrap_or(name);
    GLOBAL_CONSTANT_DEPRECATIONS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, deprecation)| *deprecation)
}

/// Returns the deprecation of `class_name::constant` when `class_name` (any case, optional
/// leading `\`) is a builtin class whose own declaration of the constant is deprecated.
pub fn class_constant_deprecation(
    class_name: &str,
    constant: &str,
) -> Option<ConstantDeprecation> {
    let class_name = class_name.strip_prefix('\\').unwrap_or(class_name);
    CLASS_CONSTANT_DEPRECATIONS
        .iter()
        .find(|entry| {
            entry.constant == constant
                && entry
                    .receivers
                    .iter()
                    .any(|receiver| receiver.eq_ignore_ascii_case(class_name))
        })
        .map(|entry| entry.deprecation)
}

/// Reports whether some builtin class declares a deprecated constant named `constant`, so a
/// backend can skip the class-hierarchy walk for every other name.
pub fn is_deprecated_class_constant_name(constant: &str) -> bool {
    CLASS_CONSTANT_DEPRECATIONS
        .iter()
        .any(|entry| entry.constant == constant)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Global lookups are case-sensitive and accept a leading namespace separator.
    #[test]
    fn global_constant_deprecations_match_exact_names() {
        let rfc7231 = global_constant_deprecation("\\DATE_RFC7231").expect("DATE_RFC7231");
        assert_eq!(rfc7231.since, PhpVersion::Php85);
        assert!(rfc7231.message.starts_with("Constant DATE_RFC7231 is deprecated since 8.5"));
        assert_eq!(
            global_constant_deprecation("MT_RAND_PHP").map(|deprecation| deprecation.since),
            Some(PhpVersion::Php83)
        );
        assert_eq!(
            global_constant_deprecation("E_STRICT").map(|deprecation| deprecation.message),
            Some("Constant E_STRICT is deprecated since 8.4, the error level was removed")
        );
        assert!(global_constant_deprecation("date_rfc7231").is_none());
        assert!(global_constant_deprecation("DATE_RFC1123").is_none());
    }

    /// Every deprecated global constant is a catalogued constant, rows are sorted and unique,
    /// each message names its constant, and its "since" matches the profile it is gated on.
    #[test]
    fn global_constant_deprecations_are_consistent() {
        for window in GLOBAL_CONSTANT_DEPRECATIONS.windows(2) {
            assert!(window[0].0 < window[1].0, "{} before {}", window[0].0, window[1].0);
        }
        for (name, deprecation) in GLOBAL_CONSTANT_DEPRECATIONS {
            assert!(crate::lookup_constant(name).is_some(), "{name}");
            assert!(deprecation.message.starts_with(&format!("Constant {name} is deprecated since ")), "{name}");
            let since = format!("deprecated since {}", deprecation.since.as_str());
            assert!(deprecation.message.contains(&since), "{name}: {since}");
        }
        assert_eq!(
            crate::lookup_constant("DATE_RFC7231").map(|constant| constant.value),
            Some(crate::ConstValue::Str(RFC7231_FORMAT))
        );
    }

    /// Class lookups ignore class-name case and report the declaring class in the message.
    #[test]
    fn class_constant_deprecations_cover_declaring_classes() {
        for receiver in ["DateTimeInterface", "\\datetime", "DATETIMEIMMUTABLE"] {
            let deprecation = class_constant_deprecation(receiver, "RFC7231").expect(receiver);
            assert!(deprecation
                .message
                .starts_with("Constant DateTimeInterface::RFC7231 is deprecated since 8.5"));
        }
        assert_eq!(
            class_constant_deprecation("pdo", "SQLITE_DETERMINISTIC").map(|deprecation| deprecation.message),
            Some("Constant PDO::SQLITE_DETERMINISTIC is deprecated since 8.5, use Pdo\\Sqlite::DETERMINISTIC instead")
        );
        assert!(class_constant_deprecation("\\Pdo\\Pgsql", "TRANSACTION_IDLE").is_some());
        assert!(class_constant_deprecation("DateTime", "rfc7231").is_none());
        assert!(class_constant_deprecation("DateTime", "RFC1123").is_none());
        assert!(class_constant_deprecation("App\\DateTime", "RFC7231").is_none());
        assert!(is_deprecated_class_constant_name("RFC7231"));
        assert!(!is_deprecated_class_constant_name("RFC1123"));
    }

    /// Every class row's message names one of its receivers and its own "since".
    #[test]
    fn class_constant_deprecations_are_consistent() {
        for entry in CLASS_CONSTANT_DEPRECATIONS {
            let declaring = entry.receivers[0];
            assert!(
                entry.deprecation.message.starts_with(&format!(
                    "Constant {declaring}::{} is deprecated since {}",
                    entry.constant,
                    entry.deprecation.since.as_str()
                )),
                "{declaring}::{}",
                entry.constant
            );
        }
    }

    /// The profile gate starts at the deprecating release.
    #[test]
    fn constant_deprecations_apply_from_their_profile() {
        let rfc7231 = global_constant_deprecation("DATE_RFC7231").expect("DATE_RFC7231");
        assert!(!rfc7231.applies_to(PhpVersion::Php84));
        assert!(rfc7231.applies_to(PhpVersion::Php85));
        assert!(rfc7231.applies_to(PhpVersion::Php86));
        assert!(!rfc7231.applies_to_version_id(80400));
        assert!(rfc7231.applies_to_version_id(80500));
    }
}
