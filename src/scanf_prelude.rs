//! Purpose:
//! PHP's `scanf` family — the engine behind `sscanf()` and `fscanf()` — implemented in
//! elephc-PHP. The two builtins keep their registry contracts and lower to the functions
//! declared here, so one engine answers both and neither needs per-target assembly.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`, after
//!   include resolution and before name resolution.
//! - `crate::builtins::string::sscanf` and `crate::builtins::io::fscanf`, whose EIR lowering
//!   emits a direct call to `__elephc_scanf` / `__elephc_fscanf`.
//!
//! Key details:
//! - WHY A PRELUDE AND NOT ASSEMBLY. The previous `__rt_sscanf` was ~700 lines of hand-written
//!   per-architecture assembly that pushed every match back as a STRING slice, so `%d` produced
//!   `string(2) "77"` where php produces `int(77)`, an unmatched conversion produced `""` where
//!   php produces `NULL`, and widths (`%5s`), suppression (`%*d`), character classes (`%[a-z]`),
//!   `%i`/`%u`/`%x`/`%o`/`%c`/`%n` and the EOF result were absent altogether. php's scanner is
//!   a byte loop with a dozen conversion arms; written in PHP it is correct on both targets at
//!   once and can be read against `ext/standard/scanf.c` line by line.
//! - PAY-FOR-USE. Injected only when `detect::program_references_scanf` finds a reference, so a
//!   program that never scans carries none of it.
//! - MEASURED, NOT GUESSED. Every rule below was read out of `php -n` 8.5.6 and is pinned by
//!   `tests/codegen/strings/scanf.rs`:
//!   * The result is `NULL` — not an array — when the scan reaches END OF INPUT before ASSIGNING
//!     anything: `sscanf('', '%d')`, `sscanf('   ', '%d')` (whitespace skipped, then EOF),
//!     `sscanf('-', '%d')` (a sign consumed, then EOF) and `sscanf('.', '%f')` are all `NULL`,
//!     while `sscanf('abc', '%d')` is `[NULL]` because the input was NOT exhausted and
//!     `sscanf('1', '%d %d')` is `[1, NULL]` because the first conversion did assign.
//!   * Scanning STOPS at the first failure and every remaining conversion contributes `NULL`:
//!     `sscanf('x 2 3', '%d %d %d')` is `[NULL, NULL, NULL]`, never `[NULL, 2, 3]`.
//!   * The array always carries one entry per NON-SUPPRESSED conversion, so a format's shape
//!     alone fixes the result's length.
//!   * `%c` takes up to `width` (default 1) NON-whitespace bytes and never skips leading ones:
//!     `sscanf(' a', '%c')` is `[""]`, `sscanf('a b', '%3c')` is `['a']`.
//!   * `%u` reads a 64-bit UNSIGNED value: `sscanf('-1', '%u')` is the STRING
//!     `'18446744073709551615'`, because the value no longer fits a PHP int.
//!   * `%i` auto-detects the base, but only when NO sign was consumed: `sscanf('0x10', '%i')`
//!     is `16` while `sscanf('-0x10', '%i')` is `0` — php scans `-0` and stops at the `x`.
//!   * Whitespace in the FORMAT matches zero or more input whitespace bytes, so
//!     `sscanf('age=42', 'age = %d')` still yields `[42]`.
//!   * A bad conversion character is a `ValueError`, raised even when scanning already stopped:
//!     `sscanf('x', '%d%q')` throws `Bad scan conversion character "q"`. `%b` is NOT a scanf
//!     conversion in php, only a printf one.
//! - THE CHARACTER CLASS IS A STRING, NOT A KEYED ARRAY. `%[a-z]` needs a byte-membership test,
//!   and building `$members[$byte] = true` would hit a live codegen defect where a NON-LITERAL
//!   integer key written into an empty array densifies it into a list — making every byte below
//!   the largest member a false positive (`%[a-z]` matched `abc123` whole). `strpos` over the
//!   expanded set has no such trap and needs no array keys at all.
//! - `fscanf()` reads ONE LINE per call through `fgets()`, newline included, exactly as php's
//!   `php_stream_get_line` does: `fscanf($h, '%[^z]')` on `"a\n"` returns `["a\n"]`. End of file
//!   is `false`; an EMPTY line is `NULL`, since scanning `"\n"` reaches EOF without assigning.
//! - BECAUSE IT REALLY CALLS `fgets()`, a diagnostic raised inside that read names `fgets`, not
//!   `fscanf`. MEASURED against a userspace wrapper with no `stream_eof`: php says
//!   `Warning: fscanf(): C::stream_eof is not implemented!` and this says `fgets()`. It is the
//!   one caller of eleven that does not name itself, and the two ways to fix it are both worse
//!   than the symptom — a global "the caller is really fscanf" pin leaks on an exception and then
//!   MIS-names other warnings, and a private `fgets`-shaped builtin that publishes `fscanf` costs
//!   a contract entry, a catalog entry and a docs regeneration for one word. Left as it is,
//!   deliberately: the warning describes what elephc actually did.
//! - THE BY-REF `$vars` FORM lives here too, as one wrapper per arity — see
//!   `scanf_vars_wrappers`. It could not before for two reasons, both now gone: the contract's
//!   `variadic: Some("vars")` was a bare NAME with no way to say the tail is written, so
//!   `fscanf($h, '%s %d', $name, $age)` was rejected for reading undefined variables
//!   (`variadic_writes` says it now, the same way `ParamSpec::writes` binds `$errno`/`$errstr`
//!   for `stream_socket_client()`); and a caller variable holding `null` did not receive a
//!   by-reference write at all, silently, which is fixed in the checker's parameter widening.
//! - THE COUNT THE `$vars` FORM ANSWERS IS NOT `count($values)`. php counts every conversion
//!   that consumed input, SUPPRESSED ones included: `sscanf('1 2 3', '%d %*d %d', $a, $b)`
//!   answers 3 while filling two variables. That number is `$assigned`, which the engine already
//!   tracks and `__elephc_scanf_ref()` now returns beside the values. Input exhausted before any
//!   conversion succeeded answers `-1` here where the array form answers `null`.

pub(crate) mod build;
mod detect;

/// The elephc-PHP scanf prelude: php's scanner plus the `sscanf`/`fscanf` entry points.
/// The PHP this engine used to be injected as, kept only as the migration oracle's reference.
///
/// `#[cfg(test)]` is the point: `build::scanf_declarations()` produces the same AST this — plus
/// the generated wrappers — parses to, and `ELEPHC_ORACLE_WHICH=scanf` compares them node by node.
/// No real compile tokenizes it any more.
#[cfg(test)]
pub(crate) const SCANF_PRELUDE_SRC: &str = r#"<?php

function __elephc_scanf_is_space(string $c): bool
{
    return $c === ' ' || $c === "\t" || $c === "\n" || $c === "\r" || $c === "\v" || $c === "\f";
}

function __elephc_scanf_digit_value(string $c): int
{
    $o = ord($c);
    if ($o >= 48 && $o <= 57) {
        return $o - 48;
    }
    if ($o >= 97 && $o <= 122) {
        return $o - 87;
    }
    if ($o >= 65 && $o <= 90) {
        return $o - 55;
    }
    return 99;
}

function __elephc_scanf_unsigned_negative(string $digits): string
{
    $minuend = '18446744073709551616';
    $out = '';
    $borrow = 0;
    $i = 0;
    $ml = strlen($minuend);
    $dl = strlen($digits);
    while ($i < $ml) {
        $da = ord($minuend[$ml - 1 - $i]) - 48;
        $db = $i < $dl ? ord($digits[$dl - 1 - $i]) - 48 : 0;
        $d = $da - $db - $borrow;
        if ($d < 0) {
            $d = $d + 10;
            $borrow = 1;
        } else {
            $borrow = 0;
        }
        $out = ((string) $d) . $out;
        $i = $i + 1;
    }
    $out = ltrim($out, '0');
    return $out === '' ? '0' : $out;
}

function __elephc_scanf_unsigned(string $digits, int $sign): int|string
{
    $ulongMax = '18446744073709551615';
    $normalized = ltrim($digits, '0');
    if ($normalized === '') {
        $normalized = '0';
    }
    $saturated = false;
    if (strlen($normalized) > 20) {
        $saturated = true;
    } elseif (strlen($normalized) === 20 && strcmp($normalized, $ulongMax) > 0) {
        $saturated = true;
    }
    if ($saturated) {
        return $ulongMax;
    }
    if ($sign < 0) {
        if ($normalized === '0') {
            return 0;
        }
        return __elephc_scanf_unsigned_negative($normalized);
    }
    $intMax = (string) PHP_INT_MAX;
    if (
        strlen($normalized) < strlen($intMax)
        || (strlen($normalized) === strlen($intMax) && strcmp($normalized, $intMax) <= 0)
    ) {
        return (int) $normalized;
    }
    return $normalized;
}

function __elephc_scanf_int(string $s, int $si, int $sl, int $width, string $conv): array
{
    $start = $si;
    $sign = 1;
    $signed = false;
    if ($si < $sl && ($s[$si] === '-' || $s[$si] === '+')) {
        $signed = true;
        if ($s[$si] === '-') {
            $sign = -1;
        }
        $si = $si + 1;
    }
    $base = 10;
    $hexPrefix = !$signed && $si + 2 < $sl && $s[$si] === '0'
        && ($s[$si + 1] === 'x' || $s[$si + 1] === 'X')
        && __elephc_scanf_digit_value($s[$si + 2]) < 16;
    if ($conv === 'x' || $conv === 'X') {
        $base = 16;
        if ($hexPrefix) {
            $si = $si + 2;
        }
    } elseif ($conv === 'o') {
        $base = 8;
    } elseif ($conv === 'i') {
        if ($hexPrefix) {
            $base = 16;
            $si = $si + 2;
        } elseif ($si < $sl && $s[$si] === '0') {
            $base = 8;
        }
    }
    $digits = '';
    while ($si < $sl) {
        if ($width > 0 && ($si - $start) >= $width) {
            break;
        }
        $d = __elephc_scanf_digit_value($s[$si]);
        if ($d >= $base) {
            break;
        }
        $digits = $digits . $s[$si];
        $si = $si + 1;
    }
    if ($digits === '') {
        return [$si, false, 0];
    }
    $magnitude = 0;
    $overflow = false;
    $i = 0;
    $len = strlen($digits);
    while ($i < $len) {
        $d = __elephc_scanf_digit_value($digits[$i]);
        if ($magnitude > intdiv(PHP_INT_MAX - $d, $base)) {
            $overflow = true;
            break;
        }
        $magnitude = $magnitude * $base + $d;
        $i = $i + 1;
    }
    if ($conv === 'u') {
        return [$si, true, __elephc_scanf_unsigned($digits, $sign)];
    }
    if ($overflow) {
        return [$si, true, $sign < 0 ? PHP_INT_MIN : PHP_INT_MAX];
    }
    return [$si, true, $sign * $magnitude];
}

function __elephc_scanf_float(string $s, int $si, int $sl, int $width): array
{
    $start = $si;
    $text = '';
    $best = '';
    $bestEnd = $si;
    $seenDigit = false;
    $seenDot = false;
    $seenExp = false;
    while ($si < $sl) {
        if ($width > 0 && ($si - $start) >= $width) {
            break;
        }
        $c = $s[$si];
        $o = ord($c);
        if ($o >= 48 && $o <= 57) {
            $seenDigit = true;
            $text = $text . $c;
            $si = $si + 1;
            $best = $text;
            $bestEnd = $si;
            continue;
        }
        if ($c === '.' && !$seenDot && !$seenExp) {
            $seenDot = true;
            $text = $text . $c;
            $si = $si + 1;
            if ($seenDigit) {
                $best = $text;
                $bestEnd = $si;
            }
            continue;
        }
        if (($c === 'e' || $c === 'E') && $seenDigit && !$seenExp) {
            $seenExp = true;
            $text = $text . $c;
            $si = $si + 1;
            continue;
        }
        $tail = $text === '' ? '' : substr($text, -1);
        if (($c === '-' || $c === '+') && ($text === '' || $tail === 'e' || $tail === 'E')) {
            $text = $text . $c;
            $si = $si + 1;
            continue;
        }
        break;
    }
    if ($best === '') {
        return [$si, false, 0.0];
    }
    return [$bestEnd, true, (float) $best];
}

function __elephc_scanf_class_members(string $body): string
{
    $members = '';
    $i = 0;
    $len = strlen($body);
    while ($i < $len) {
        $c = $body[$i];
        if ($c === '-' && $i > 0 && $i + 1 < $len) {
            $from = ord($body[$i - 1]);
            $to = ord($body[$i + 1]);
            if ($to >= $from) {
                $k = $from;
                while ($k <= $to) {
                    $members = $members . chr($k);
                    $k = $k + 1;
                }
                $i = $i + 2;
                continue;
            }
        }
        $members = $members . $c;
        $i = $i + 1;
    }
    return $members;
}

function __elephc_scanf_is_conversion(string $conv): bool
{
    return $conv === 'c' || $conv === 'd' || $conv === 'D' || $conv === 'e' || $conv === 'E'
        || $conv === 'f' || $conv === 'g' || $conv === 'i' || $conv === 'n' || $conv === 'o'
        || $conv === 's' || $conv === 'u' || $conv === 'x' || $conv === 'X';
}

function __elephc_scanf_ref(string $s, string $fmt): array
{
    $sl = strlen($s);
    $fl = strlen($fmt);
    $si = 0;
    $fi = 0;
    $values = [];
    $assigned = 0;
    $eof = false;
    $stop = false;

    while ($fi < $fl && !$stop && !$eof) {
        $fc = $fmt[$fi];
        if (__elephc_scanf_is_space($fc)) {
            $fi = $fi + 1;
            while ($si < $sl && __elephc_scanf_is_space($s[$si])) {
                $si = $si + 1;
            }
            continue;
        }
        if ($fc !== '%') {
            if ($si >= $sl) {
                $eof = true;
                break;
            }
            if ($s[$si] !== $fc) {
                $stop = true;
                break;
            }
            $si = $si + 1;
            $fi = $fi + 1;
            continue;
        }
        $fi = $fi + 1;
        $suppress = false;
        if ($fi < $fl && $fmt[$fi] === '*') {
            $suppress = true;
            $fi = $fi + 1;
        }
        $width = 0;
        while ($fi < $fl && ord($fmt[$fi]) >= 48 && ord($fmt[$fi]) <= 57) {
            $width = $width * 10 + (ord($fmt[$fi]) - 48);
            $fi = $fi + 1;
        }
        if ($fi < $fl && ($fmt[$fi] === 'l' || $fmt[$fi] === 'h' || $fmt[$fi] === 'L')) {
            $fi = $fi + 1;
        }
        $conv = $fi < $fl ? $fmt[$fi] : "\0";
        $fi = $fi + 1;

        if ($conv === '%') {
            if ($si >= $sl) {
                $eof = true;
                break;
            }
            if ($s[$si] !== '%') {
                $stop = true;
                break;
            }
            $si = $si + 1;
            continue;
        }

        $class = '';
        $negated = false;
        if ($conv === '[') {
            $body = '';
            if ($fi < $fl && $fmt[$fi] === '^') {
                $negated = true;
                $fi = $fi + 1;
            }
            if ($fi < $fl && $fmt[$fi] === ']') {
                $body = ']';
                $fi = $fi + 1;
            }
            $closed = false;
            while ($fi < $fl) {
                if ($fmt[$fi] === ']') {
                    $closed = true;
                    $fi = $fi + 1;
                    break;
                }
                $body = $body . $fmt[$fi];
                $fi = $fi + 1;
            }
            if (!$closed) {
                throw new \ValueError('Unmatched [ in format string');
            }
            $class = __elephc_scanf_class_members($body);
        } elseif (!__elephc_scanf_is_conversion($conv)) {
            throw new \ValueError('Bad scan conversion character "' . $conv . '"');
        }

        if ($conv === 'n') {
            $assigned = $assigned + 1;
            if (!$suppress) {
                $values[] = $si;
            }
            continue;
        }

        if ($conv !== 'c' && $conv !== '[') {
            while ($si < $sl && __elephc_scanf_is_space($s[$si])) {
                $si = $si + 1;
            }
        }
        if ($si >= $sl) {
            $eof = true;
            if (!$suppress) {
                $values[] = null;
            }
            break;
        }

        $ok = false;
        $value = null;
        if ($conv === 's') {
            $start = $si;
            while ($si < $sl && !__elephc_scanf_is_space($s[$si])) {
                if ($width > 0 && ($si - $start) >= $width) {
                    break;
                }
                $si = $si + 1;
            }
            $ok = $si > $start;
            $value = substr($s, $start, $si - $start);
        } elseif ($conv === 'c') {
            $take = $width > 0 ? $width : 1;
            $start = $si;
            while ($si < $sl && !__elephc_scanf_is_space($s[$si]) && ($si - $start) < $take) {
                $si = $si + 1;
            }
            $ok = true;
            $value = substr($s, $start, $si - $start);
        } elseif ($conv === '[') {
            $start = $si;
            while ($si < $sl) {
                if ($width > 0 && ($si - $start) >= $width) {
                    break;
                }
                $inside = strpos($class, $s[$si]) !== false;
                if ($negated) {
                    $inside = !$inside;
                }
                if (!$inside) {
                    break;
                }
                $si = $si + 1;
            }
            $ok = $si > $start;
            $value = substr($s, $start, $si - $start);
        } elseif ($conv === 'e' || $conv === 'E' || $conv === 'f' || $conv === 'g') {
            $r = __elephc_scanf_float($s, $si, $sl, $width);
            $si = $r[0];
            $ok = $r[1];
            $value = $r[2];
        } else {
            $r = __elephc_scanf_int($s, $si, $sl, $width, $conv);
            $si = $r[0];
            $ok = $r[1];
            $value = $r[2];
        }

        if (!$ok) {
            if ($si >= $sl) {
                $eof = true;
            } else {
                $stop = true;
            }
            if (!$suppress) {
                $values[] = null;
            }
            break;
        }
        $assigned = $assigned + 1;
        if (!$suppress) {
            $values[] = $value;
        }
    }

    while ($fi < $fl) {
        $fc = $fmt[$fi];
        if ($fc !== '%') {
            $fi = $fi + 1;
            continue;
        }
        $fi = $fi + 1;
        $suppress = false;
        if ($fi < $fl && $fmt[$fi] === '*') {
            $suppress = true;
            $fi = $fi + 1;
        }
        while ($fi < $fl && ord($fmt[$fi]) >= 48 && ord($fmt[$fi]) <= 57) {
            $fi = $fi + 1;
        }
        if ($fi < $fl && ($fmt[$fi] === 'l' || $fmt[$fi] === 'h' || $fmt[$fi] === 'L')) {
            $fi = $fi + 1;
        }
        $conv = $fi < $fl ? $fmt[$fi] : "\0";
        $fi = $fi + 1;
        if ($conv === '%') {
            continue;
        }
        if ($conv === '[') {
            if ($fi < $fl && $fmt[$fi] === '^') {
                $fi = $fi + 1;
            }
            if ($fi < $fl && $fmt[$fi] === ']') {
                $fi = $fi + 1;
            }
            $closed = false;
            while ($fi < $fl) {
                if ($fmt[$fi] === ']') {
                    $closed = true;
                    $fi = $fi + 1;
                    break;
                }
                $fi = $fi + 1;
            }
            if (!$closed) {
                throw new \ValueError('Unmatched [ in format string');
            }
        } elseif (!__elephc_scanf_is_conversion($conv)) {
            throw new \ValueError('Bad scan conversion character "' . $conv . '"');
        }
        if (!$suppress) {
            $values[] = null;
        }
    }

    $exhausted = $eof && $assigned === 0;
    return [$exhausted ? -1 : $assigned, $values, $exhausted ? 1 : 0, count($values)];
}

function __elephc_scanf(string $s, string $fmt): array|null
{
    $r = __elephc_scanf_ref($s, $fmt);
    if ($r[2] === 1) {
        return null;
    }
    $values = $r[1];
    $n = (int) $r[3];
    $out = [];
    for ($i = 0; $i < $n; $i++) {
        $out[] = $values[$i];
    }
    return $out;
}

function __elephc_fscanf(mixed $stream, string $format): array|false|null
{
    $line = fgets($stream);
    if ($line === false) {
        return false;
    }
    return __elephc_scanf($line, $format);
}

function __elephc_scanf_arity(int $found, int $wanted): void
{
    if ($found > $wanted) {
        throw new \ValueError('Different numbers of variable names and field specifiers');
    }
    if ($found < $wanted) {
        throw new \ValueError('Variable is not assigned by any conversion specifiers');
    }
}
"#;

/// The largest `$vars` count the by-reference `sscanf()`/`fscanf()` form accepts.
///
/// php takes any number. Each one here is a distinct PHP function with that many by-reference
/// parameters, because this backend has no way to write THROUGH the elements of a by-reference
/// variadic — `&...$vars` collects addresses into an array and a write to `$vars[$i]` replaces
/// the address rather than following it. Eight covers every scanf call the PHP manual and the
/// wild show; past it the builtin refuses with a message that names the limit rather than
/// scanning into nothing.
pub(crate) const SCANF_MAX_VARS: usize = 8;

/// Builds the by-reference `$vars` wrappers, one pair per arity.
///
/// Each wrapper runs the shared engine once, checks the variable count against the format's
/// conversion count the way php does, assigns every variable — including the ones no conversion
/// reached, which php sets to `null` — and answers the count php answers.
///
/// That count is NOT `count($values)`: php counts every conversion that consumed input, the
/// SUPPRESSED ones included, so `sscanf("1 2 3", "%d %*d %d", $a, $b)` answers 3 while handing
/// back two variables. The engine already tracks exactly that number; it is the first element of
/// what `__elephc_scanf_ref()` returns. Input exhausted before any conversion succeeded is `-1`,
/// where the array form answers `null`. Both measured on `php -n` 8.5.6.
#[cfg(test)]
fn scanf_vars_wrappers() -> String {
    let mut out = String::new();
    for count in 1..=SCANF_MAX_VARS {
        // DECLARED `mixed`, not left untyped. An untyped by-reference parameter takes its type
        // from the call site, and a prelude function is resolved before any call site is seen —
        // so it fell back to the `int` placeholder, the caller handed over a Mixed cell pointer,
        // and the callee wrote an int through it. The declaration is what pins the two together.
        let params: String = (0..count)
            .map(|index| format!(", mixed &$v{index}"))
            .collect();
        // The element goes through an OWNED local before it crosses the reference. Assigning
        // `$vals[$i]` straight to `&$vN` stored a BORROWED pointer into the caller's slot, and
        // the callee's own cleanup then freed the array it pointed into — the caller read freed
        // memory and the program segfaulted on the first use of the variable.
        let assigns: String = (0..count)
            .map(|index| {
                format!("    $t{index} = $vals[{index}];\n    $v{index} = $t{index};\n")
            })
            .collect();
        out.push('\n');
        out.push_str(&format!(
            "function __elephc_scanf_vars_{count}(string $s, string $fmt{params}): int\n"
        ));
        out.push_str("{\n");
        out.push_str("    $r = __elephc_scanf_ref($s, $fmt);\n");
        out.push_str("    $vals = $r[1];\n");
        out.push_str(&format!(
            "    __elephc_scanf_arity((int) $r[3], {count});\n"
        ));
        out.push_str(&assigns);
        out.push_str("    return (int) $r[0];\n");
        out.push_str("}\n");
        out.push('\n');
        out.push_str(&format!(
            "function __elephc_fscanf_vars_{count}(mixed $stream, string $fmt{params}): int|false\n"
        ));
        out.push_str("{\n");
        out.push_str("    $line = fgets($stream);\n");
        out.push_str("    if ($line === false) {\n");
        out.push_str("        return false;\n");
        out.push_str("    }\n");
        out.push_str("    $r = __elephc_scanf_ref($line, $fmt);\n");
        out.push_str("    $vals = $r[1];\n");
        out.push_str(&format!(
            "    __elephc_scanf_arity((int) $r[3], {count});\n"
        ));
        out.push_str(&assigns);
        out.push_str("    return (int) $r[0];\n");
        out.push_str("}\n");
    }
    out
}

/// The reachability group these declarations belong to.
///
/// It is FORCED wherever the pass runs, which is not a hole in pay-for-use: the prelude is
/// injected only when the program references `sscanf`/`fscanf`, so the group existing at all is
/// already the proof that the program needs every function in it.
pub const PRELUDE_GROUP_ID: &str = "scanf";

/// Injects the scanf prelude when the program references `sscanf()` or `fscanf()`, leaving
/// every other program untouched.
///
/// There is no user-declaration escape hatch here, unlike `dir()`: both names are registry
/// builtins, so PHP itself refuses to redeclare them and no program can own them. The prelude
/// carries only declarations, so prepending it is order-independent — PHP hoists them.
pub fn inject_if_used(
    program: crate::parser::ast::Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> crate::parser::ast::Program {
    if !detect::program_references_scanf(&program) {
        return program;
    }
    // BUILT, not parsed. `build::scanf_declarations` produces the same AST the PHP under
    // `#[cfg(test)]` below — engine plus generated wrappers — parses to, and the migration oracle
    // compares them node by node, so neither the tokenizer nor the parser runs over this surface.
    let mut combined = build::scanf_declarations();
    // These declarations are reachable ONLY through a call the backend emits for `sscanf()` and
    // `fscanf()`. No walk over PHP source can see that edge, so without recording the group here
    // — and forcing it where the pass runs — reachability prunes the engine and the backend then
    // meets a call to a function that no longer exists.
    inventory.record_program(PRELUDE_GROUP_ID, &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod build_oracle_tests {
    /// Verifies the BUILT declarations are the same AST the PHP form parses to.
    ///
    /// This is what makes the conversion trustworthy, and it runs on every test run rather than
    /// on request: a builder nothing compares drifts, and a PHP reference nothing reads rots into
    /// dead code. The comparison strips spans, because the two constructions cannot agree on
    /// source positions and never needed to.
    #[test]
    fn built_declarations_match_the_php_form() {
        let source = format!("{}{}", super::SCANF_PRELUDE_SRC, super::scanf_vars_wrappers());
        let tokens = crate::lexer::tokenize(&source).expect("the PHP form must tokenize");
        let parsed = crate::parser::parse_internal(&tokens).expect("the PHP form must parse");
        let built = super::build::scanf_declarations();
        assert_eq!(
            built.len(),
            parsed.len(),
            "declaration count: built {} vs parsed {}",
            built.len(),
            parsed.len()
        );
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            assert_eq!(
                crate::synthetic_class::transcribe::strip_spans(&format!("{built_stmt:?}")),
                crate::synthetic_class::transcribe::strip_spans(&format!("{parsed_stmt:?}")),
            );
        }
    }
}
