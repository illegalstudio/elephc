//! Purpose:
//! Central registry for PDO drivers compiled into the elephc database bridge.
//!
//! Called from:
//! - `crate` connection dispatch, driver-name attributes, and availability exports.
//!
//! Key details:
//! - Registry order is PHP-visible through `pdo_drivers()` and remains stable.
//! - New optional drivers must add one variant and one `AVAILABLE` entry here.

/// Identifies a PDO backend compiled into this bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DriverKind {
    #[cfg(feature = "dblib")]
    Dblib,
    #[cfg(feature = "firebird")]
    Firebird,
    #[cfg(feature = "odbc")]
    Odbc,
    Mysql,
    Pgsql,
    Sqlite,
}

/// Drivers exposed to PHP, in the stable order used by the existing bridge.
pub(crate) const AVAILABLE: &[DriverKind] = &[
    #[cfg(feature = "dblib")]
    DriverKind::Dblib,
    #[cfg(feature = "firebird")]
    DriverKind::Firebird,
    #[cfg(feature = "odbc")]
    DriverKind::Odbc,
    DriverKind::Mysql,
    DriverKind::Pgsql,
    DriverKind::Sqlite,
];

impl DriverKind {
    /// Returns the lowercase PDO driver name exposed by PHP.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            #[cfg(feature = "dblib")]
            Self::Dblib => "dblib",
            #[cfg(feature = "firebird")]
            Self::Firebird => "firebird",
            #[cfg(feature = "odbc")]
            Self::Odbc => "odbc",
            Self::Mysql => "mysql",
            Self::Pgsql => "pgsql",
            Self::Sqlite => "sqlite",
        }
    }

    /// Returns the DSN prefix, including its separating colon.
    pub(crate) const fn dsn_prefix(self) -> &'static str {
        match self {
            #[cfg(feature = "dblib")]
            Self::Dblib => "dblib:",
            #[cfg(feature = "firebird")]
            Self::Firebird => "firebird:",
            #[cfg(feature = "odbc")]
            Self::Odbc => "odbc:",
            Self::Mysql => "mysql:",
            Self::Pgsql => "pgsql:",
            Self::Sqlite => "sqlite:",
        }
    }

    /// Selects a compiled driver from a full colon-bearing DSN.
    pub(crate) fn from_dsn(dsn: &str) -> Option<Self> {
        AVAILABLE
            .iter()
            .copied()
            .find(|driver| dsn.starts_with(driver.dsn_prefix()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps the PHP-visible availability order stable.
    #[test]
    fn available_driver_order_is_stable() {
        let names: Vec<_> = AVAILABLE.iter().map(|driver| driver.name()).collect();
        #[cfg(not(any(feature = "dblib", feature = "firebird", feature = "odbc")))]
        assert_eq!(names, ["mysql", "pgsql", "sqlite"]);
        #[cfg(all(feature = "dblib", not(feature = "firebird"), not(feature = "odbc")))]
        assert_eq!(names, ["dblib", "mysql", "pgsql", "sqlite"]);
        #[cfg(all(not(feature = "dblib"), feature = "firebird", not(feature = "odbc")))]
        assert_eq!(names, ["firebird", "mysql", "pgsql", "sqlite"]);
        #[cfg(all(feature = "dblib", feature = "firebird", not(feature = "odbc")))]
        assert_eq!(names, ["dblib", "firebird", "mysql", "pgsql", "sqlite"]);
        #[cfg(all(not(feature = "dblib"), not(feature = "firebird"), feature = "odbc"))]
        assert_eq!(names, ["odbc", "mysql", "pgsql", "sqlite"]);
        #[cfg(all(feature = "dblib", not(feature = "firebird"), feature = "odbc"))]
        assert_eq!(names, ["dblib", "odbc", "mysql", "pgsql", "sqlite"]);
        #[cfg(all(not(feature = "dblib"), feature = "firebird", feature = "odbc"))]
        assert_eq!(names, ["firebird", "odbc", "mysql", "pgsql", "sqlite"]);
        #[cfg(all(feature = "dblib", feature = "firebird", feature = "odbc"))]
        assert_eq!(names, ["dblib", "firebird", "odbc", "mysql", "pgsql", "sqlite"]);
    }

    /// Dispatches only exact lowercase PDO prefixes followed by a colon.
    #[test]
    fn dsn_dispatch_requires_exact_registered_prefix() {
        assert_eq!(DriverKind::from_dsn("sqlite::memory:"), Some(DriverKind::Sqlite));
        assert_eq!(DriverKind::from_dsn("pgsql:host=localhost"), Some(DriverKind::Pgsql));
        assert_eq!(DriverKind::from_dsn("mysql:host=localhost"), Some(DriverKind::Mysql));
        #[cfg(feature = "dblib")]
        assert_eq!(DriverKind::from_dsn("dblib:host=localhost"), Some(DriverKind::Dblib));
        #[cfg(feature = "firebird")]
        assert_eq!(DriverKind::from_dsn("firebird:dbname=test.fdb"), Some(DriverKind::Firebird));
        #[cfg(feature = "odbc")]
        assert_eq!(DriverKind::from_dsn("odbc:example"), Some(DriverKind::Odbc));
        assert_eq!(DriverKind::from_dsn("SQLite::memory:"), None);
        assert_eq!(DriverKind::from_dsn("sqlite"), None);
    }
}
