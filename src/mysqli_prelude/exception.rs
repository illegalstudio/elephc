//! Purpose:
//! The `mysqli_sql_exception` class and the `mysqli_report()` flag store, as an
//! elephc-PHP source fragment. A mysqli-only program must never see (or throw)
//! `PDOException`; this exception is the only one the surface raises.
//!
//! Called from:
//! - `crate::mysqli_prelude::fragments::source_for_version` (concatenated into
//!   the injected prelude).
//!
//! Key details:
//! - `mysqli_sql_exception extends RuntimeException` with a public `$sqlstate`,
//!   matching php-src's shape (there `$sqlstate` is protected with a getter; the
//!   public property is the documented elephc divergence).
//! - The process-wide report mode lives on `mysqli::$reportMode` (see
//!   `connection.rs`); its default is version-gated in `fragments.rs`
//!   (`ERROR|STRICT` = 3 for PHP >= 8.1, `OFF` = 0 for 8.0).

/// The `mysqli_sql_exception` class and `mysqli_report()` (fragment without a
/// `<?php` header).
pub(super) const SRC: &str = r#"
// -- mysqli error surface --

// php-src: class mysqli_sql_exception extends RuntimeException. The SQLSTATE is
// exposed as a public property (php-src keeps it protected behind getSqlState();
// documented divergence, same bucket as writable mysqli properties).
class mysqli_sql_exception extends RuntimeException {
    public string $sqlstate = "00000";

    // The previous exception in the chain, stored in Exception's inherited Throwable slot.
    public ?Throwable $previous = null;

    public function __construct(string $message = "", int $code = 0, ?Throwable $previous = null) {
        // The built-in Exception constructor is a checker-synthesized method
        // with no linkable symbol, so `parent::__construct()` cannot be called;
        // the public $message property is assigned directly (same pattern as
        // PDOException in the PDO prelude).
        $this->message = $message;
        $this->code = $code;
        $this->previous = $previous;
    }

    // php 8.1+: the documented accessor for the SQLSTATE (the property itself is
    // protected in php-src; elephc exposes both — see the class note).
    public function getSqlState(): string {
        return $this->sqlstate;
    }
}

// Stores the process-wide report mode consumed by every mysqli failure path.
// PHP 8.1+ default: MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT; PHP 8.0: OFF
// (the default itself is baked into mysqli::$reportMode at injection time).
function mysqli_report(int $flags): bool {
    mysqli::$reportMode = $flags;
    return true;
}
"#;
