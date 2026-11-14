//! Purpose:
//! Declares PHP's core `pack()`/`unpack()` family, including the PHP 8.6
//! endianness modifiers and unpack element names.
//!
//! Called from:
//! - `crate::core_prelude::inject_if_used` when a program calls `pack`/`unpack` or
//!   names them dynamically.
//!
//! Key details:
//! - Format parsing, string padding, integer narrowing, and byte order live in PHP so every
//!   target shares one implementation. The `elephc_pack` bridge supplies only the IEEE-754
//!   binary32/binary64 bit conversions, which PHP cannot express at the integer level.
//! - The prelude is parsed before name resolution, so the extern block it carries makes the
//!   type checker record the bridge as a required library only for programs that use pack.
//! - Endianness modifiers are consumed only for the PHP 8.6 profile. On earlier profiles the
//!   `<`/`>` character remains an unknown format code, matching those runtimes.

use crate::compiler_stack::with_compiler_stack;
use crate::parser::ast::Program;

/// The complete pack/unpack implementation, with a placeholder for the 8.6 gate.
const PACK_PRELUDE_SRC: &str = r#"<?php

extern "elephc_pack" {
    function elephc_pack_f64_bits(float $value): u64;
    function elephc_pack_bits_f64(u64 $bits): float;
    function elephc_pack_f32_bits(float $value): u32;
    function elephc_pack_bits_f32(u32 $bits): float;
}

function pack(string $format, mixed ...$values): string {
    $out = '';
    $formatLength = strlen($format);
    $valueCount = count($values);
    $valueIndex = 0;
    $index = 0;
    while ($index < $formatLength) {
        $code = $format[$index];
        $index++;
        $endian = '';
        if (__ELEPHC_PACK_ENDIAN_ALLOWED__ && $index < $formatLength
            && ($format[$index] === '<' || $format[$index] === '>')) {
            $endian = $format[$index];
            $index++;
        }
        $repeat = 1;
        $star = false;
        if ($index < $formatLength && $format[$index] === '*') {
            $star = true;
            $index++;
        } elseif ($index < $formatLength && $format[$index] >= '0' && $format[$index] <= '9') {
            $digits = '';
            while ($index < $formatLength && $format[$index] >= '0' && $format[$index] <= '9') {
                $digits .= $format[$index];
                $index++;
            }
            $repeat = (int) $digits;
        }

        if ($endian !== '') {
            __elephc_pack_validate_endian($code, $endian);
        }

        switch ($code) {
            case 'x':
                __elephc_pack_reject_endian($code, $endian);
                if ($star) { $repeat = 1; }
                $out .= str_repeat("\0", $repeat);
                break;
            case 'X':
                __elephc_pack_reject_endian($code, $endian);
                if ($star) { $repeat = 1; }
                if ($repeat > strlen($out)) {
                    trigger_error('pack(): Type X: outside of string', E_USER_WARNING);
                    $out = '';
                } else {
                    $out = substr($out, 0, strlen($out) - $repeat);
                }
                break;
            case '@':
                __elephc_pack_reject_endian($code, $endian);
                if ($star) { $repeat = 1; }
                if ($repeat < strlen($out)) {
                    $out = substr($out, 0, $repeat);
                } else {
                    $out .= str_repeat("\0", $repeat - strlen($out));
                }
                break;
            case 'a':
            case 'A':
            case 'Z':
                __elephc_pack_reject_endian($code, $endian);
                if ($valueIndex >= $valueCount) {
                    throw new \ValueError('Type ' . $code . ': not enough arguments');
                }
                $text = (string) $values[$valueIndex];
                $valueIndex++;
                if ($star) {
                    $repeat = strlen($text);
                    if ($code === 'Z') { $repeat++; }
                }
                if ($code === 'Z') {
                    $nul = strpos($text, "\0");
                    if ($nul !== false) { $text = substr($text, 0, $nul); }
                    if ($repeat === 0) {
                        // Z0 is empty.
                    } elseif (strlen($text) >= $repeat) {
                        $out .= substr($text, 0, $repeat - 1) . "\0";
                    } else {
                        $out .= $text . str_repeat("\0", $repeat - strlen($text));
                    }
                } elseif (strlen($text) >= $repeat) {
                    $out .= substr($text, 0, $repeat);
                } else {
                    $padding = $code === 'A' ? ' ' : "\0";
                    $out .= $text . str_repeat($padding, $repeat - strlen($text));
                }
                break;
            case 'h':
            case 'H':
                __elephc_pack_reject_endian($code, $endian);
                if ($valueIndex >= $valueCount) {
                    throw new \ValueError('Type ' . $code . ': not enough arguments');
                }
                $text = (string) $values[$valueIndex];
                $valueIndex++;
                $nibbles = $star ? strlen($text) : $repeat;
                if ($nibbles > strlen($text) * 2) {
                    trigger_error('pack(): Type ' . $code . ': not enough characters in string', E_USER_WARNING);
                }
                $out .= __elephc_pack_hex($text, $nibbles, $code === 'H');
                break;
            case 'c':
            case 'C':
            case 's':
            case 'S':
            case 'n':
            case 'v':
            case 'i':
            case 'I':
            case 'l':
            case 'L':
            case 'N':
            case 'V':
            case 'q':
            case 'Q':
            case 'J':
            case 'P':
                if ($star) {
                    $repeat = $valueCount - $valueIndex;
                }
                if ($valueIndex + $repeat > $valueCount) {
                    throw new \ValueError('Type ' . $code . ': too few arguments');
                }
                for ($i = 0; $i < $repeat; $i++) {
                    $value = $values[$valueIndex];
                    if (is_float($value) && ($value >= 9223372036854775808.0 || $value < -9223372036854775808.0 || is_nan($value))) {
                        trigger_error('The float ' . sprintf('%.16G', $value) . ' is not representable as an int, cast occurred', E_USER_WARNING);
                    }
                    $value = (int) $value;
                    $valueIndex++;
                    $out .= __elephc_pack_int_code($code, $value, $endian);
                }
                break;
            case 'f':
            case 'g':
            case 'G':
                if ($star) {
                    $repeat = $valueCount - $valueIndex;
                }
                if ($valueIndex + $repeat > $valueCount) {
                    throw new \ValueError('Type ' . $code . ': too few arguments');
                }
                for ($i = 0; $i < $repeat; $i++) {
                    $value = (float) $values[$valueIndex];
                    $valueIndex++;
                    $bits = elephc_pack_f32_bits($value);
                    $little = $code !== 'G' && $endian !== '>';
                    $out .= $little ? __elephc_pack_int_le($bits, 4) : __elephc_pack_int_be($bits, 4);
                }
                break;
            case 'd':
            case 'e':
            case 'E':
                if ($star) {
                    $repeat = $valueCount - $valueIndex;
                }
                if ($valueIndex + $repeat > $valueCount) {
                    throw new \ValueError('Type ' . $code . ': too few arguments');
                }
                for ($i = 0; $i < $repeat; $i++) {
                    $value = (float) $values[$valueIndex];
                    $valueIndex++;
                    $bits = elephc_pack_f64_bits($value);
                    $little = $code !== 'E' && $endian !== '>';
                    $out .= $little ? __elephc_pack_int_le($bits, 8) : __elephc_pack_int_be($bits, 8);
                }
                break;
            default:
                throw new \ValueError('Type ' . $code . ': unknown format code');
        }
    }
    if ($valueIndex < $valueCount) {
        trigger_error('pack(): ' . ($valueCount - $valueIndex) . ' arguments unused', E_USER_WARNING);
    }
    return $out;
}

function unpack(string $format, string $string, int $offset = 0): array|false {
    $inputLength = strlen($string);
    if ($offset < 0 || $offset > $inputLength) {
        throw new \ValueError('unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)');
    }
    $position = 0;
    $dataLength = $inputLength - $offset;
    $result = [];
    $formatLength = strlen($format);
    $index = 0;
    $overflow = false;
    while ($index < $formatLength) {
        $code = $format[$index];
        $index++;
        $endian = '';
        if (__ELEPHC_PACK_ENDIAN_ALLOWED__ && $index < $formatLength
            && ($format[$index] === '<' || $format[$index] === '>')) {
            $endian = $format[$index];
            $index++;
        }
        $repeat = 1;
        $star = false;
        if ($index < $formatLength && $format[$index] === '*') {
            $star = true;
            $index++;
        } elseif ($index < $formatLength && $format[$index] >= '0' && $format[$index] <= '9') {
            $digits = '';
            while ($index < $formatLength && $format[$index] >= '0' && $format[$index] <= '9') {
                $digits .= $format[$index];
                $index++;
            }
            $overflow = strlen($digits) > 10 || (strlen($digits) === 10 && strcmp($digits, '2147483647') > 0);
            $repeat = (int) $digits;
        }
        $nameStart = $index;
        while ($index < $formatLength && $format[$index] !== '/') {
            $index++;
        }
        $name = substr($format, $nameStart, $index - $nameStart);
        if ($index < $formatLength && $format[$index] === '/') {
            $index++;
        }

        if ($endian !== '') {
            __elephc_pack_validate_endian($code, $endian);
        }
        if ($overflow) {
            trigger_error('unpack(): Type ' . $code . ': integer overflow', E_USER_WARNING);
            return false;
        }

        switch ($code) {
            case 'a':
            case 'A':
            case 'Z':
                __elephc_pack_reject_endian($code, $endian);
                $take = $star ? $dataLength - $position : $repeat;
                if ($take < 0 || $position + $take > $dataLength) {
                    __elephc_pack_warn_short($code, $take, $dataLength - $position);
                    return false;
                }
                $value = substr($string, $offset + $position, $take);
                if ($code === 'A') {
                    $value = rtrim($value, "\0 \t\r\n");
                } elseif ($code === 'Z') {
                    $nul = strpos($value, "\0");
                    if ($nul !== false) { $value = substr($value, 0, $nul); }
                }
                $position += $take;
                __elephc_pack_store($result, $name, $repeat, 0, $value);
                break;
            case 'h':
            case 'H':
                __elephc_pack_reject_endian($code, $endian);
                $nibbles = $star ? ($dataLength - $position) * 2 : $repeat;
                $bytes = intdiv($nibbles + 1, 2);
                if ($position + $bytes > $dataLength) {
                    __elephc_pack_warn_short($code, $bytes, $dataLength - $position);
                    return false;
                }
                $hex = '';
                for ($i = 0; $i < $nibbles; $i++) {
                    $byte = ord($string[$offset + $position + intdiv($i, 2)]);
                    if ($code === 'H') {
                        $nibble = ($i % 2 === 0) ? ($byte >> 4) : ($byte & 0x0F);
                    } else {
                        $nibble = ($i % 2 === 0) ? ($byte & 0x0F) : ($byte >> 4);
                    }
                    $hex .= dechex($nibble);
                }
                $position += $bytes;
                __elephc_pack_store($result, $name, 1, 0, $hex);
                break;
            case 'x':
                __elephc_pack_reject_endian($code, $endian);
                if ($star) { $repeat = 1; }
                for ($i = 0; $i < $repeat; $i++) {
                    if ($position + 1 > $dataLength) {
                        __elephc_pack_warn_short($code, 1, 0);
                        return false;
                    }
                    $position++;
                }
                break;
            case 'X':
                __elephc_pack_reject_endian($code, $endian);
                if ($star) { $repeat = 1; }
                $position = max(0, $position - $repeat);
                break;
            case '@':
                __elephc_pack_reject_endian($code, $endian);
                if ($star) { $repeat = 1; }
                if ($repeat <= $dataLength) {
                    $position = $repeat;
                } else {
                    trigger_error('unpack(): Type @: outside of string', E_USER_WARNING);
                }
                break;
            case 'c':
            case 'C':
            case 's':
            case 'S':
            case 'n':
            case 'v':
            case 'i':
            case 'I':
            case 'l':
            case 'L':
            case 'N':
            case 'V':
            case 'q':
            case 'Q':
            case 'J':
            case 'P':
                $size = __elephc_pack_int_size($code);
                $count = $star ? intdiv($dataLength - $position, $size) : $repeat;
                for ($i = 0; $i < $count; $i++) {
                    if ($position + $size > $dataLength) {
                        __elephc_pack_warn_short($code, $size, $dataLength - $position);
                        return false;
                    }
                    $signed = __elephc_pack_int_signed($code);
                    $little = __elephc_pack_int_little($code, $endian);
                    $value = __elephc_pack_read_int($string, $offset + $position, $size, $little, $signed);
                    if ($value === false) { return false; }
                    $position += $size;
                    __elephc_pack_store($result, $name, $count, $i, $value);
                }
                break;
            case 'f':
            case 'g':
            case 'G':
                $size = 4;
                $count = $star ? intdiv($dataLength - $position, $size) : $repeat;
                for ($i = 0; $i < $count; $i++) {
                    if ($position + $size > $dataLength) {
                        __elephc_pack_warn_short($code, $size, $dataLength - $position);
                        return false;
                    }
                    $little = $code !== 'G' && $endian !== '>';
                    $bits = __elephc_pack_read_int($string, $offset + $position, $size, $little, false);
                    if ($bits === false) { return false; }
                    $position += $size;
                    __elephc_pack_store($result, $name, $count, $i, elephc_pack_bits_f32($bits));
                }
                break;
            case 'd':
            case 'e':
            case 'E':
                $size = 8;
                $count = $star ? intdiv($dataLength - $position, $size) : $repeat;
                for ($i = 0; $i < $count; $i++) {
                    if ($position + $size > $dataLength) {
                        __elephc_pack_warn_short($code, $size, $dataLength - $position);
                        return false;
                    }
                    $little = $code !== 'E' && $endian !== '>';
                    $bits = __elephc_pack_read_int($string, $offset + $position, $size, $little, false);
                    if ($bits === false) { return false; }
                    $position += $size;
                    __elephc_pack_store($result, $name, $count, $i, elephc_pack_bits_f64($bits));
                }
                break;
            default:
                throw new \ValueError('Invalid format type ' . $code);
        }
    }
    return $result;
}

function __elephc_pack_warn_short(string $code, int $need, int $have): void {
    $verb = $have === 1 ? 'was' : 'were';
    trigger_error('unpack(): Type ' . $code . ': not enough input values, need ' . $need . ' values but only ' . $have . ' ' . $verb . ' provided', E_USER_WARNING);
}

function __elephc_pack_validate_endian(string $code, string $endian): void {
    if (strpos('nNvVJPGgeE', $code) !== false) {
        throw new \ValueError("Endianness modifier '" . $endian . "' cannot be applied to format code '" . $code . "' which already has inherent endianness");
    }
    if (strpos('sSlLqQfd', $code) === false) {
        throw new \ValueError("Endianness modifier is not supported for format code '" . $code . "'");
    }
}

function __elephc_pack_reject_endian(string $code, string $endian): void {
    if ($endian !== '') {
        throw new \ValueError("Endianness modifier is not supported for format code '" . $code . "'");
    }
}

function __elephc_pack_int_code(string $code, int $value, string $endian): string {
    $little = __elephc_pack_int_little($code, $endian);
    $size = __elephc_pack_int_size($code);
    return $little ? __elephc_pack_int_le($value, $size) : __elephc_pack_int_be($value, $size);
}

function __elephc_pack_int_size(string $code): int {
    if ($code === 'c' || $code === 'C') { return 1; }
    if ($code === 's' || $code === 'S' || $code === 'n' || $code === 'v') { return 2; }
    if ($code === 'q' || $code === 'Q' || $code === 'J' || $code === 'P') { return 8; }
    return 4;
}

function __elephc_pack_int_signed(string $code): bool {
    return $code === 'c' || $code === 's' || $code === 'i' || $code === 'l' || $code === 'q';
}

function __elephc_pack_int_little(string $code, string $endian): bool {
    if ($endian === '<') { return true; }
    if ($endian === '>') { return false; }
    if ($code === 'n' || $code === 'N' || $code === 'J') { return false; }
    if ($code === 'v' || $code === 'V' || $code === 'P') { return true; }
    return true;
}

function __elephc_pack_int_le(int $value, int $bytes): string {
    $out = '';
    for ($i = 0; $i < $bytes; $i++) {
        $out .= chr(($value >> (8 * $i)) & 0xFF);
    }
    return $out;
}

function __elephc_pack_int_be(int $value, int $bytes): string {
    $out = '';
    for ($i = $bytes - 1; $i >= 0; $i--) {
        $out .= chr(($value >> (8 * $i)) & 0xFF);
    }
    return $out;
}

function __elephc_pack_read_int(string $data, int $position, int $bytes, bool $little, bool $signed): int|false {
    if ($position < 0 || $position + $bytes > strlen($data)) {
        return false;
    }
    if ($bytes === 8) {
        $low = 0;
        if ($little) {
            for ($i = 0; $i < 7; $i++) {
                $low |= ord($data[$position + $i]) << (8 * $i);
            }
            $high = ord($data[$position + 7]);
        } else {
            $high = ord($data[$position]);
            for ($i = 0; $i < 7; $i++) {
                $low = ($low << 8) | ord($data[$position + $i + 1]);
            }
        }
        $signedHigh = $high >= 128 ? $high - 256 : $high;
        return $low | ($signedHigh << 56);
    }
    $value = 0;
    if ($little) {
        for ($i = 0; $i < $bytes; $i++) {
            $value |= ord($data[$position + $i]) << (8 * $i);
        }
    } else {
        for ($i = 0; $i < $bytes; $i++) {
            $value = ($value << 8) | ord($data[$position + $i]);
        }
    }
    if ($signed) {
        $sign = 1 << (8 * $bytes - 1);
        if (($value & $sign) !== 0) {
            $value -= 1 << (8 * $bytes);
        }
    }
    return $value;
}

function __elephc_pack_hex(string $text, int $nibbles, bool $highFirst): string {
    $out = '';
    for ($i = 0; $i < $nibbles; $i += 2) {
        $high = __elephc_pack_nibble($text, $i);
        $low = __elephc_pack_nibble($text, $i + 1);
        $out .= chr($highFirst ? (($high << 4) | $low) : (($low << 4) | $high));
    }
    return $out;
}

function __elephc_pack_nibble(string $text, int $index): int {
    if ($index >= strlen($text)) {
        return 0;
    }
    $digit = strpos('0123456789abcdef', strtolower($text[$index]));
    return $digit === false ? 0 : $digit;
}

function __elephc_pack_store(array &$result, string $name, int $count, int $index, mixed $value): void {
    if ($name === '') {
        $result[$index + 1] = $value;
    } elseif ($count === 1) {
        $result[$name] = $value;
    } else {
        $result[$name . ($index + 1)] = $value;
    }
}
"#;

/// Returns whether `usage` reaches `pack()` or `unpack()`.
pub(crate) fn requested(usage: &crate::prelude_prune::usage::Usage) -> bool {
    usage
        .functions
        .iter()
        .any(|name| name == "pack" || name == "unpack")
        || usage
            .literals
            .iter()
            .any(|name| name.eq_ignore_ascii_case("pack") || name.eq_ignore_ascii_case("unpack"))
}

/// Returns the parsed `pack()`/`unpack()` declarations for one PHP profile.
pub(crate) fn declarations(php_version: crate::php_version::PhpVersion) -> Program {
    with_compiler_stack(|| {
        let allow_endian = php_version >= crate::php_version::PhpVersion::Php86;
        let source = PACK_PRELUDE_SRC.replace(
            "__ELEPHC_PACK_ENDIAN_ALLOWED__",
            if allow_endian { "true" } else { "false" },
        );
        let tokens = crate::lexer::tokenize(&source).expect("pack prelude must tokenize");
        crate::parser::parse_internal(&tokens).expect("pack prelude must parse")
    })
}
