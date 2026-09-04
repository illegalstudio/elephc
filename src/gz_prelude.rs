//! Purpose:
//! PHP's `gz*` stream surface — `gzopen`, `gzread`, `gzgets`, `gzwrite`, `gzseek`, `gzfile`,
//! `readgzfile` and their siblings — implemented in elephc-PHP on top of the `compress.zlib://`
//! wrapper the stream functions already serve. All fourteen were absent: a program calling
//! `gzopen($f, "r")` failed with "Undefined function".
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`, after include
//!   resolution and before name resolution.
//!
//! Key details:
//! - THE FOUR STRING FUNCTIONS (`gzencode`, `gzdecode`, `zlib_encode`, `zlib_decode`) are here too,
//!   though they frame BYTES rather than serve a stream: they are the rest of what ext-zlib owes
//!   that the primitives already present can build. `ZLIB_ENCODING_RAW` and `_DEFLATE` were
//!   MEASURED to be exactly `gzdeflate()` and `gzcompress()`, so those encodings ARE those calls;
//!   only the gzip framing is written out, and its body is exactly `gzdeflate($data, $level)`.
//! - `$data` IS DECLARED `mixed` on those four, and has to be. `gzdecode(gzencode($s))` is
//!   idiomatic php — the inner call answers `string|false` and php complains only at RUN time, if
//!   a false actually arrives. These are ordinary PHP functions here, so the checker applies
//!   user-function strictness and refused the CALL: "parameter $data expects Str, got
//!   Union([Str, False])". The same nesting through the native `gzuncompress(gzcompress($s))`
//!   compiles, because a builtin's own check hook tolerates it — the strictness is about where a
//!   function is declared, not about these functions. The catalogue still declares `string`, which
//!   is what php documents.
//! - THE GZIP OS BYTE is the one value that cannot be measured for every target from one host:
//!   zlib stamps its own `OS_CODE`, 19 on Darwin and 3 on other Unix builds. It is selected from
//!   `PHP_OS` so each target stamps what its own zlib would, and no test asserts it — the
//!   round trip and the platform-independent header and trailer bytes are what is pinned.
//! - WHY THIS IS AN EQUIVALENCE AND NOT AN APPROXIMATION. php-src implements `gzopen` as a stream
//!   open on the zlib wrapper, so the whole family IS the plain stream API over that URL. That was
//!   MEASURED rather than read: all fifteen pairs below — `gzread`/`fread`, `gzgets`/`fgets`,
//!   `gzgetc`/`fgetc`, `gzeof`/`feof`, `gzseek`/`fseek` in both `SEEK_SET` and `SEEK_CUR` forms,
//!   `gzrewind`/`rewind`, `gztell`/`ftell`, `gzpassthru`/`fpassthru`, `gzfile`/`file`,
//!   `readgzfile`/`readfile`, `gzwrite`/`fwrite`, and a failed open — answer identically under
//!   `php -n` 8.5.6. elephc's own answer to the wrapper spelling was then measured against php on
//!   the same fifteen: 15 of 15 match, so the prelude inherits nothing wrong.
//! - THE MODE PASSES THROUGH UNTOUCHED. `gzopen($f, "wb9")` carries a compression LEVEL and
//!   STRATEGY that plain `fopen` never sees; `fopen("compress.zlib://…", "wb9")` and `"wb1f"` were
//!   measured to answer exactly what `gzopen` does with the same mode, so no parsing is needed
//!   here.
//! - `$stream` IS DECLARED `mixed`, and has to be. An UNTYPED parameter infers as `int` here, and
//!   the stream builtins refuse that with "expects resource, got int" — raised in the bodies of
//!   the functions the program never calls, which are checked all the same. `mixed` is what
//!   `ensure_stream_resource` accepts for a value it cannot narrow, and is what the directory
//!   prelude already passes to `readdir()` for the same reason.
//! - `?int $length = null` IS BRANCHED ON, not forwarded. php's `gzgets($h, null)` reads a whole
//!   line, and forwarding the null would make it a length. The branch is what keeps the two
//!   spellings apart at the one place that knows they differ.
//! - `$use_include_path` IS FORWARDED, not accepted and ignored. php honours it on all three of
//!   `gzopen`, `gzfile` and `readgzfile`, and dropping it would also leave the parameter unread —
//!   which elephc reports as `Unused variable: $use_include_path` and refuses the compile over,
//!   so the prelude cannot carry a decorative parameter even if that were acceptable.
//! - PAY-FOR-USE. Injected only when `detect::program_uses_gz` finds a reference, so a program
//!   that never touches a gzip stream carries none of this.
//! - A program that declares its OWN function of one of these names suppresses injection. Not for
//!   php-fidelity — php FATALS with "Cannot redeclare gzopen()" there — but because elephc emits
//!   BOTH declarations and the ASSEMBLER stops the build: MEASURED, a user `function foo()` plus
//!   an `if (!function_exists("foo"))` redeclaration fails with
//!   `error: symbol '_fn_foo' is already defined`, where php answers the first one. Suppressing
//!   keeps such a program compiling instead of ending in an assembler diagnostic.
//!   The `function_exists`-guarded polyfill shape still does not compile, for a SEPARATE and
//!   pre-existing reason — a conditionally declared function is not registered, so the call
//!   reports "Undefined function" — and that is unchanged by this prelude either way.
//! - MEASURED DIVERGENCE, and the price of implementing a builtin in PHP: a failed open warns in
//!   the words of the call this is BUILT from, at the line that call sits on. `gzopen("nope.gz",
//!   "r")` on line 1 answers `Warning: fopen(nope.gz): Failed to open stream: No such file or
//!   directory ... on line 4` where php says `Warning: gzopen(nope.gz): ... on line 1`. The VALUE
//!   — `false` — matches, and warning with a wrong name and line was preferred to the alternative
//!   of suppressing the inner warning and going SILENT, which is the failure mode with no
//!   diagnostic at all.
//! - A SECOND divergence lives one level down and is not this prelude's: elephc's own
//!   `fopen("compress.zlib://nope.gz", "r")` warns `fopen(nope.gz): ... No such file or directory`
//!   where php warns `fopen(compress.zlib://nope.gz): ... operation failed`, naming the URL rather
//!   than the underlying path. Fixing it there fixes it here.

pub(crate) mod build;
mod detect;

/// The elephc-PHP gzip-stream prelude.
///
/// Every body is one existing builtin call on a `compress.zlib://` URL, so the whole surface
/// compiles through the ordinary function pipeline with NO new assembly and both architectures get
/// it at once.
/// The PHP this surface used to be injected as, kept only as the migration oracle's reference.
///
/// `#[cfg(test)]` is the whole point: `build::gz_declarations()` produces the same AST this parses
/// to — `ELEPHC_ORACLE_WHICH=gz` compares them node by node — so no real compile tokenizes it any
/// more. It stays because a builder with nothing to be checked against is a builder nobody can
/// trust.
#[cfg(test)]
pub(crate) const GZ_PRELUDE_SRC: &str = r#"<?php

function gzopen(string $filename, string $mode, int $use_include_path = 0) {
    return fopen('compress.zlib://' . $filename, $mode, $use_include_path !== 0);
}

function gzclose(mixed $stream): bool {
    return fclose($stream);
}

function gzeof(mixed $stream): bool {
    return feof($stream);
}

function gzgetc(mixed $stream): string|false {
    return fgetc($stream);
}

function gzgets(mixed $stream, ?int $length = null): string|false {
    if ($length === null) {
        return fgets($stream);
    }
    return fgets($stream, $length);
}

function gzread(mixed $stream, int $length): string|false {
    return fread($stream, $length);
}

function gzwrite(mixed $stream, string $data, ?int $length = null): int|false {
    if ($length === null) {
        return fwrite($stream, $data);
    }
    return fwrite($stream, $data, $length);
}

function gzputs(mixed $stream, string $data, ?int $length = null): int|false {
    return gzwrite($stream, $data, $length);
}

function gzpassthru(mixed $stream): int {
    return fpassthru($stream);
}

function gzrewind(mixed $stream): bool {
    return rewind($stream);
}

function gzseek(mixed $stream, int $offset, int $whence = SEEK_SET): int {
    return fseek($stream, $offset, $whence);
}

function gztell(mixed $stream): int|false {
    return ftell($stream);
}

function __elephc_gzip_frame(mixed $data, int $level): string {
    // MEASURED on `php -n` 8.5.6: magic, deflate method, no flags, zero mtime, then XFL and OS.
    $xfl = 0;
    if ($level === 9) {
        $xfl = 2;
    } elseif ($level === 0 || $level === 1) {
        $xfl = 4;
    }
    // zlib stamps its own OS_CODE: 19 on Darwin, 3 on other Unix builds.
    $os = PHP_OS === 'Darwin' ? 19 : 3;
    $crc = crc32($data);
    $len = strlen($data);
    return "\x1f\x8b\x08\x00\x00\x00\x00\x00" . chr($xfl) . chr($os)
        . gzdeflate($data, $level)
        . chr($crc & 255) . chr(($crc >> 8) & 255) . chr(($crc >> 16) & 255) . chr(($crc >> 24) & 255)
        . chr($len & 255) . chr(($len >> 8) & 255) . chr(($len >> 16) & 255) . chr(($len >> 24) & 255);
}

function gzencode(mixed $data, int $level = -1, int $encoding = 31): string|false {
    if ($level < -1 || $level > 9) {
        throw new \ValueError('gzencode(): Argument #2 ($level) must be between -1 and 9');
    }
    if ($encoding === -15) {
        return gzdeflate($data, $level);
    }
    if ($encoding === 15) {
        return gzcompress($data, $level);
    }
    if ($encoding !== 31) {
        throw new \ValueError('gzencode(): Argument #3 ($encoding) must be one of ZLIB_ENCODING_RAW, ZLIB_ENCODING_GZIP, or ZLIB_ENCODING_DEFLATE');
    }
    return __elephc_gzip_frame($data, $level);
}

function zlib_encode(mixed $data, int $encoding, int $level = -1): string|false {
    if ($level < -1 || $level > 9) {
        throw new \ValueError('zlib_encode(): Argument #3 ($level) must be between -1 and 9');
    }
    if ($encoding === -15) {
        return gzdeflate($data, $level);
    }
    if ($encoding === 15) {
        return gzcompress($data, $level);
    }
    if ($encoding !== 31) {
        throw new \ValueError('zlib_encode(): Argument #2 ($encoding) must be one of ZLIB_ENCODING_RAW, ZLIB_ENCODING_GZIP, or ZLIB_ENCODING_DEFLATE');
    }
    return __elephc_gzip_frame($data, $level);
}

function __elephc_gzip_body(mixed $data): string|false {
    // The header is fixed-width only when no optional field is present; FLG says which are.
    if (strlen($data) < 18) {
        return false;
    }
    if ($data[0] !== "\x1f" || $data[1] !== "\x8b" || ord($data[2]) !== 8) {
        return false;
    }
    $flg = ord($data[3]);
    $pos = 10;
    if (($flg & 4) !== 0) {
        $pos = $pos + 2 + (ord($data[$pos]) | (ord($data[$pos + 1]) << 8));
    }
    if (($flg & 8) !== 0) {
        $nameEnd = strpos($data, "\x00", $pos);
        if ($nameEnd === false) {
            return false;
        }
        $pos = $nameEnd + 1;
    }
    if (($flg & 16) !== 0) {
        $commentEnd = strpos($data, "\x00", $pos);
        if ($commentEnd === false) {
            return false;
        }
        $pos = $commentEnd + 1;
    }
    if (($flg & 2) !== 0) {
        $pos = $pos + 2;
    }
    $length = strlen($data) - $pos - 8;
    if ($length < 0) {
        return false;
    }
    return substr($data, $pos, $length);
}

function gzdecode(mixed $data, int $max_length = 0): string|false {
    $body = __elephc_gzip_body($data);
    if ($body === false) {
        return false;
    }
    return gzinflate($body, $max_length);
}

function zlib_decode(mixed $data, int $max_length = 0): string|false {
    if (strlen($data) < 2) {
        return false;
    }
    // The three framings are told apart by their first bytes, which is what php's own
    // auto-detection does: gzip carries its magic, a zlib stream a CMF whose low nibble is 8.
    if ($data[0] === "\x1f" && $data[1] === "\x8b") {
        return gzdecode($data, $max_length);
    }
    if ((ord($data[0]) & 15) === 8) {
        return gzuncompress($data, $max_length);
    }
    return gzinflate($data, $max_length);
}

function zlib_get_coding_type(): string|false {
    // php reports what its OUTPUT layer compressed with, and answers false when nothing did.
    // elephc has no zlib output compression, so false is php's answer for every configuration
    // this can be in — not a placeholder for one.
    return false;
}

function gzfile(string $filename, int $use_include_path = 0): array|false {
    return file('compress.zlib://' . $filename, $use_include_path !== 0 ? FILE_USE_INCLUDE_PATH : 0);
}

function readgzfile(string $filename, int $use_include_path = 0): int|false {
    return readfile('compress.zlib://' . $filename, $use_include_path !== 0);
}
"#;

/// Injects the gzip-stream prelude when the program references one of its functions, leaving every
/// other program untouched.
///
/// The prelude carries only declarations, so prepending it is order-independent — PHP hoists them.
pub fn inject_if_used(program: crate::parser::ast::Program) -> crate::parser::ast::Program {
    if !detect::program_uses_gz(&program) || detect::program_declares_gz(&program) {
        return program;
    }
    // BUILT, not parsed. `build::gz_declarations` produces the same AST the PHP under
    // `#[cfg(test)]` below parses to — the migration oracle compares them node by node — so the
    // tokenizer and parser no longer run over this surface on every compile that touches it.
    let mut combined = build::gz_declarations();
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
        let tokens = crate::lexer::tokenize(super::GZ_PRELUDE_SRC).expect("the PHP form must tokenize");
        let parsed = crate::parser::parse_internal(&tokens).expect("the PHP form must parse");
        let built = super::build::gz_declarations();
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
