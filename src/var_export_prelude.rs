//! Purpose:
//! Injects the PHP `var_export()` standard-library function (written in elephc-PHP)
//! that renders a parsable representation of a scalar or array value, matching the
//! interpreter's layout: `'…'`-quoted strings, `true`/`false`/`NULL` keywords, and
//! the indented `array ( … )` form with `key => value,` entries.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`,
//!   before name resolution, so a user `var_export(...)` call resolves to the
//!   injected function through the normal pipeline (functions, recursion, arrays,
//!   string builtins) with no dedicated codegen or runtime helper.
//!
//! Key details:
//! - Implemented as a prelude rather than a runtime walker because the recursive,
//!   string-building format reuses ordinary PHP control flow; this keeps it correct
//!   on every supported target with no per-target assembly.
//! - Pay-for-use: injected only when `detect::program_references_var_export` finds a
//!   call or a `"var_export"` string (covering `function_exists`/callable forms), and
//!   never when the program already declares its own `var_export` (so user
//!   definitions win and there is no redeclaration conflict).
//! - Floats render with the interpreter's `serialize_precision = -1` semantics: the
//!   shortest decimal string that round-trips back to the same `double`, formatted
//!   with PHP's decimal/scientific layout (`1.0`, `0.3333333333333333`, `1.0E+17`,
//!   `1.0E-6`). `__elephc_var_export_float` finds the shortest precision by probing
//!   `sprintf("%.{p}e", ...)` until `(float)` of the result equals the input, then
//!   rebuilds the digit string per PHP's exponent thresholds — independent of the
//!   default `(string)`/`echo` precision used elsewhere.
//! - Objects are out of scope (PHP renders `\Class::__set_state(...)`); a non
//!   scalar/array value renders as the empty string.

use crate::parser::ast::Program;

mod detect;

/// The elephc-PHP `var_export` prelude: the public `var_export($value, $return)`
/// entry point plus two internal helpers (`__elephc_var_export_str` renders a value
/// to its parsable text, `__elephc_var_export_escape` single-quote-escapes a string).
/// The helpers are prefixed so they cannot collide with user code, and `var_export`
/// itself is injected only when the user does not define their own.
pub const VAR_EXPORT_PRELUDE_SRC: &str = r#"<?php
function __elephc_var_export_escape(mixed $s): string {
    $s = (string) $s;
    return str_replace("'", "\\'", str_replace("\\", "\\\\", $s));
}
function __elephc_var_export_float(float $f): string {
    if (is_nan($f)) {
        return 'NAN';
    }
    if (is_infinite($f)) {
        return $f < 0 ? '-INF' : 'INF';
    }
    if ($f === 0.0) {
        return ((string) $f)[0] === '-' ? '-0.0' : '0.0';
    }
    $s = '';
    for ($p = 0; $p <= 16; $p++) {
        $s = sprintf("%.{$p}e", $f);
        if ((float) $s === $f) {
            break;
        }
    }
    $start = ($s[0] === '-') ? 1 : 0;
    $neg = $start === 1;
    $epos = strpos($s, 'e');
    $exp = (int) substr($s, $epos + 1);
    $digits = str_replace('.', '', substr($s, $start, $epos - $start));
    $ndigits = strlen($digits);
    $decpt = $exp + 1;
    if ($decpt < -3 || $decpt > 17) {
        $out = $digits[0];
        $out = $out . (($ndigits > 1) ? '.' . substr($digits, 1) : '.0');
        $e = $decpt - 1;
        $out = $out . 'E' . ($e >= 0 ? '+' : '-') . abs($e);
    } else if ($decpt <= 0) {
        $out = '0.' . str_repeat('0', -$decpt) . $digits;
    } else if ($decpt >= $ndigits) {
        $out = $digits . str_repeat('0', $decpt - $ndigits) . '.0';
    } else {
        $out = substr($digits, 0, $decpt) . '.' . substr($digits, $decpt);
    }
    return ($neg ? '-' : '') . $out;
}
function __elephc_var_export_str(mixed $value, int $indent): string {
    if (is_int($value)) {
        return (string) $value;
    }
    if (is_float($value)) {
        return __elephc_var_export_float((float) $value);
    }
    if (is_bool($value)) {
        return $value ? 'true' : 'false';
    }
    if (is_null($value)) {
        return 'NULL';
    }
    if (is_string($value)) {
        return "'" . __elephc_var_export_escape($value) . "'";
    }
    if (is_array($value)) {
        $pad = str_repeat(' ', $indent);
        $out = "array (\n";
        foreach ($value as $k => $v) {
            if (is_int($k)) {
                $key = (string) $k;
            } else {
                $key = "'" . __elephc_var_export_escape($k) . "'";
            }
            $out = $out . $pad . '  ' . $key . ' => ';
            if (is_array($v)) {
                $out = $out . "\n" . $pad . '  ' . __elephc_var_export_str($v, $indent + 2);
            } else {
                $out = $out . __elephc_var_export_str($v, $indent + 2);
            }
            $out = $out . ",\n";
        }
        $out = $out . $pad . ')';
        return $out;
    }
    return '';
}
function var_export(mixed $value, bool $return = false) {
    $rendered = __elephc_var_export_str($value, 0);
    if ($return) {
        return $rendered;
    }
    echo $rendered;
    return null;
}
"#;

/// Prepends the `var_export` prelude when the program references `var_export` and does
/// not declare its own, so unrelated binaries pay nothing and a user definition is not
/// clobbered. The prelude is hoisted function declarations only, so prepending does not
/// change top-level execution order. The source is static and tested, so a
/// tokenize/parse failure is a compiler bug and panics rather than degrading silently.
pub fn inject_if_used(program: Program) -> Program {
    if !detect::program_references_var_export(&program)
        || detect::program_declares_var_export(&program)
    {
        return program;
    }
    let tokens = crate::lexer::tokenize(VAR_EXPORT_PRELUDE_SRC).expect("var_export prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("var_export prelude must parse");
    combined.extend(program);
    combined
}
