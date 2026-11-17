//! Purpose:
//! The `mysqli_driver` and `mysqli_warning` classes, as an elephc-PHP source
//! fragment (the parse-parity oracle for `build::driver`).
//!
//! Called from:
//! - `crate::mysqli_prelude::fragments::source_for_version` (concatenated into
//!   the injected prelude).
//!
//! Key details:
//! - `mysqli_driver`'s properties are virtual property hooks over process-wide
//!   state: `report_mode` reads and writes the same `mysqli::$reportMode` that
//!   `mysqli_report()` sets, like php's property handlers.
//! - `mysqli_warning` has a private constructor (php's
//!   "Cannot directly construct"); the connection / statement build it from
//!   `SHOW WARNINGS` rows through a private factory, and `next()` walks the chain.

/// The `mysqli_driver` and `mysqli_warning` classes (fragment without a
/// `<?php` header).
pub(super) const SRC: &str = r##"
// -- mysqli driver and warning objects --

// php-src: final class mysqli_driver. Its properties are live views of the
// process-wide client state: report_mode reads and writes the same mode
// mysqli_report() sets.
final class mysqli_driver {
    public string $client_info {
        get => elephc_pdo_mysql_client_info();
    }

    public int $client_version {
        get => mysqli_get_client_version();
    }

    public int $driver_version {
        get => 101009;
    }

    public int $report_mode {
        get => mysqli::$reportMode;
        set {
            mysqli::$reportMode = $value;
        }
    }
}

// php-src: final class mysqli_warning — one SHOW WARNINGS row, with next()
// advancing through the rest of the chain.
final class mysqli_warning {
    public string $message = "";
    public string $sqlstate = "HY000";
    public int $errno = 0;

    // The whole SHOW WARNINGS result (codes and messages, in order) and the
    // index of the row the public properties currently describe.
    private array $codes = [];
    private array $messages = [];
    private int $position = 0;

    private function __construct() {
    }

    // Internal factory used by mysqli::get_warnings() / mysqli_stmt::get_warnings().
    private static function __elephcFromRows(array $codes, array $messages): mysqli_warning {
        $_warning = new mysqli_warning();
        $_warning->codes = $codes;
        $_warning->messages = $messages;
        $_warning->load(0);
        return $_warning;
    }

    public function next(): bool {
        if ($this->position + 1 >= count($this->codes)) {
            return false;
        }
        $this->load($this->position + 1);
        return true;
    }

    // Points the public properties at warning row $index.
    private function load(int $index): void {
        $this->position = $index;
        $this->errno = (int) $this->codes[$index];
        $this->message = (string) $this->messages[$index];
        $this->sqlstate = "HY000";
    }
}
"##;
