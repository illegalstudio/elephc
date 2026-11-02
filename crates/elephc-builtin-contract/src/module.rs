//! Purpose:
//! Names the PHP module (php-src extension) that owns each shared function, class,
//! and constant contract, plus the `Elephc` pseudo-module for surfaces PHP does not have.
//!
//! Called from:
//! - Shared function, class, and constant catalogs (`module` field).
//! - `tools/gen_builtins.rs`, which exports the module name for the compatibility page.
//!
//! Key details:
//! - Variants mirror php-src's bundled `ext/` directory as PHP itself reports it through
//!   `ReflectionExtension` / `ReflectionFunction::getExtensionName()`, lowercased, plus the
//!   two surfaces Reflection names differently: `core` (Zend) and `zend opcache`.
//! - `php_name()` is the exact lowercase Reflection spelling, which is also the key the
//!   vendored `scripts/docs/php_baseline.json` snapshot uses, so a catalog module can be
//!   cross-checked against PHP mechanically.
//! - `Elephc` marks elephc-only surfaces (`ptr_*`, `buffer_*`, `zval_*`, `__elephc_*`
//!   internals); they are never counted against a PHP module.

macro_rules! php_modules {
    ($( $(#[$meta:meta])* $variant:ident => $name:literal ),* $(,)?) => {
        /// PHP module (php-src bundled extension) that owns a shared symbol contract.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum PhpModule {
            $( $(#[$meta])* $variant, )*
            /// elephc-specific surface with no PHP counterpart.
            Elephc,
        }

        impl PhpModule {
            /// Every module in declaration order, `Elephc` last.
            pub const ALL: &'static [PhpModule] = &[ $( Self::$variant, )* Self::Elephc ];

            /// Returns the lowercase name PHP's Reflection API reports for this module.
            pub const fn php_name(self) -> &'static str {
                match self {
                    $( Self::$variant => $name, )*
                    Self::Elephc => "elephc",
                }
            }
        }
    };
}

php_modules! {
    /// Zend engine surface (`ReflectionFunction::getExtensionName()` returns `false`).
    Core => "core",
    ZendOpcache => "zend opcache",
    Bcmath => "bcmath",
    Bz2 => "bz2",
    Calendar => "calendar",
    Ctype => "ctype",
    Curl => "curl",
    Date => "date",
    Dba => "dba",
    Dom => "dom",
    Enchant => "enchant",
    Exif => "exif",
    Ffi => "ffi",
    Fileinfo => "fileinfo",
    Filter => "filter",
    Ftp => "ftp",
    Gd => "gd",
    Gettext => "gettext",
    Gmp => "gmp",
    Hash => "hash",
    Iconv => "iconv",
    Intl => "intl",
    Json => "json",
    Ldap => "ldap",
    Lexbor => "lexbor",
    Libxml => "libxml",
    Mbstring => "mbstring",
    Mysqli => "mysqli",
    Mysqlnd => "mysqlnd",
    Odbc => "odbc",
    Openssl => "openssl",
    Pcntl => "pcntl",
    Pcre => "pcre",
    Pdo => "pdo",
    PdoDblib => "pdo_dblib",
    PdoFirebird => "pdo_firebird",
    PdoMysql => "pdo_mysql",
    PdoOdbc => "pdo_odbc",
    PdoPgsql => "pdo_pgsql",
    PdoSqlite => "pdo_sqlite",
    Pgsql => "pgsql",
    Phar => "phar",
    Posix => "posix",
    Random => "random",
    Readline => "readline",
    Reflection => "reflection",
    Session => "session",
    Shmop => "shmop",
    Simplexml => "simplexml",
    Snmp => "snmp",
    Soap => "soap",
    Sockets => "sockets",
    Sodium => "sodium",
    Spl => "spl",
    Sqlite3 => "sqlite3",
    Standard => "standard",
    Sysvmsg => "sysvmsg",
    Sysvsem => "sysvsem",
    Sysvshm => "sysvshm",
    Tidy => "tidy",
    Tokenizer => "tokenizer",
    Uri => "uri",
    Xml => "xml",
    Xmlreader => "xmlreader",
    Xmlwriter => "xmlwriter",
    Xsl => "xsl",
    Zip => "zip",
    Zlib => "zlib",
    /// PECL `imagick` (not bundled with php-src); provided by the image prelude.
    Imagick => "imagick",
    /// PECL `gmagick` (not bundled with php-src); provided by the image prelude.
    Gmagick => "gmagick",
    /// PECL `cairo` (not bundled with php-src); provided by the image prelude.
    Cairo => "cairo",
    /// PECL `pdo_ibm` (not bundled with php-src); provided by the PDO prelude.
    PdoIbm => "pdo_ibm",
}

impl PhpModule {
    /// Finds a module by its Reflection name, case-insensitively (`"Core"`, `"SPL"`, `"Zend OPcache"`).
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|module| module.php_name().eq_ignore_ascii_case(name))
    }

    /// Returns the name PHP registers this module under, which is the spelling a Reflection dump
    /// prints (`<internal:SPL>`) and `get_loaded_extensions()` lists. It differs from the lowercase
    /// `php_name()` key only for the modules below. Measured with `get_loaded_extensions()` on a
    /// PHP 8.5.10 build that loads the bundled modules, `uri` and `lexbor` included; the four PECL
    /// modules (`imagick`, `gmagick`, `cairo`, `pdo_ibm`) were not loaded there, so their lowercase
    /// spelling is taken from their own sources rather than measured.
    pub const fn registered_name(self) -> &'static str {
        match self {
            Self::Core => "Core",
            Self::Spl => "SPL",
            Self::Ffi => "FFI",
            Self::ZendOpcache => "Zend OPcache",
            Self::Pdo => "PDO",
            Self::PdoOdbc => "PDO_ODBC",
            Self::Phar => "Phar",
            Self::Reflection => "Reflection",
            Self::Simplexml => "SimpleXML",
            _ => self.php_name(),
        }
    }

    /// Returns whether this module is a real PHP module rather than the elephc pseudo-module.
    pub const fn is_php(self) -> bool {
        !matches!(self, Self::Elephc)
    }

    /// Returns whether php-src bundles this module. PECL modules elephc happens to provide
    /// (`imagick`, `gmagick`, `cairo`, `pdo_ibm`) are real PHP modules but never appear in
    /// the vendored php-src baseline, so coverage pages report them separately.
    pub const fn is_bundled(self) -> bool {
        !matches!(
            self,
            Self::Elephc | Self::Imagick | Self::Gmagick | Self::Cairo | Self::PdoIbm
        )
    }
}

#[cfg(test)]
mod tests {
    use super::PhpModule;

    /// Verifies Reflection spellings round-trip and are unique.
    #[test]
    fn php_names_are_unique_and_round_trip() {
        let mut seen = std::collections::HashSet::new();
        for module in PhpModule::ALL {
            assert!(seen.insert(module.php_name()), "duplicate module name {}", module.php_name());
            assert_eq!(PhpModule::parse(module.php_name()), Some(*module));
            assert_eq!(module.php_name(), module.php_name().to_ascii_lowercase());
        }
        assert_eq!(PhpModule::parse("Zend OPcache"), Some(PhpModule::ZendOpcache));
        assert_eq!(PhpModule::parse("SPL"), Some(PhpModule::Spl));
        assert_eq!(PhpModule::parse("nope"), None);
        assert!(!PhpModule::Elephc.is_php());
        assert_eq!(PhpModule::ALL.len(), 68 + 4 + 1);
        assert!(!PhpModule::Imagick.is_bundled() && PhpModule::Imagick.is_php());
    }

    /// Verifies each registered spelling is only a recasing of the module's key.
    #[test]
    fn registered_names_recase_the_reflection_key() {
        for module in PhpModule::ALL {
            assert_eq!(module.registered_name().to_ascii_lowercase(), module.php_name());
            assert_eq!(PhpModule::parse(module.registered_name()), Some(*module));
        }
        assert_eq!(PhpModule::Standard.registered_name(), "standard");
    }

    /// Pins every recased spelling, since the round-trip above cannot catch a wrong capital.
    #[test]
    fn registered_names_match_php_spelling() {
        let expected = [
            (PhpModule::Core, "Core"),
            (PhpModule::Spl, "SPL"),
            (PhpModule::Ffi, "FFI"),
            (PhpModule::ZendOpcache, "Zend OPcache"),
            (PhpModule::Pdo, "PDO"),
            (PhpModule::PdoOdbc, "PDO_ODBC"),
            (PhpModule::Phar, "Phar"),
            (PhpModule::Reflection, "Reflection"),
            (PhpModule::Simplexml, "SimpleXML"),
        ];
        for (module, name) in expected {
            assert_eq!(module.registered_name(), name);
        }
        let recased = PhpModule::ALL
            .iter()
            .filter(|module| module.registered_name() != module.php_name())
            .count();
        assert_eq!(recased, expected.len(), "a recased module is missing from this list");
    }
}
