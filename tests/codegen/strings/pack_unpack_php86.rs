//! Purpose:
//! Runs PHP 8.6 `pack()`/`unpack()` fixtures, including endianness modifiers and names.
//!
//! Called from:
//! - `cargo test --test codegen_tests pack_unpack_php86`.
//!
//! Key details:
//! - Sources are transcribed from `ext/standard/tests/strings/pack*.phpt` and
//!   `unpack*.phpt`. The float cases compare exact hex bytes produced by the bridge.
//! - The pack/unpack implementation is a pay-for-use PHP prelude plus the small
//!   `elephc_pack` IEEE-754 bridge.
//! - PHP warnings are not asserted; fixtures compare stdout values and ValueError messages.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// Compiles and runs one fixture with the default PHP 8.5 profile.
fn run85(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php85)
}

/// php-src `pack_endian_modifiers.phpt`, integer and float subset.
#[test]
fn test_pack_endian_modifiers_php86() {
    let out = run86(
        r#"<?php
var_dump(bin2hex(pack('s<', 0x0102)));
var_dump(bin2hex(pack('S<', 0x0102)));
var_dump(bin2hex(pack('s>', 0x0102)));
var_dump(bin2hex(pack('S>', 0x0102)));
var_dump(pack('s<', 0x0102) === pack('v', 0x0102));
var_dump(pack('s>', 0x0102) === pack('n', 0x0102));
var_dump(bin2hex(pack('l<', 0x01020304)));
var_dump(bin2hex(pack('L<', 0x01020304)));
var_dump(bin2hex(pack('l>', 0x01020304)));
var_dump(bin2hex(pack('L>', 0x01020304)));
var_dump(pack('l<', 0x01020304) === pack('V', 0x01020304));
var_dump(pack('l>', 0x01020304) === pack('N', 0x01020304));
var_dump(unpack('s<', "\x02\x01"));
var_dump(unpack('s>', "\x01\x02"));
var_dump(unpack('s<', "\xfe\xff"));
var_dump(unpack('s>', "\xff\xfe"));
var_dump(unpack('l<', "\xfe\xff\xff\xff"));
var_dump(unpack('l>', "\xff\xff\xff\xfe"));
var_dump(bin2hex(pack('s<2', 0x0102, 0x0304)));
var_dump(unpack('s<2', "\x02\x01\x04\x03"));
var_dump(unpack('s<value/L>count', "\x02\x01\x00\x00\x00\x05"));
var_dump(pack('f<', 3.14) === pack('g', 3.14));
var_dump(pack('f>', 3.14) === pack('G', 3.14));
var_dump(pack('d<', 3.14) === pack('e', 3.14));
var_dump(pack('d>', 3.14) === pack('E', 3.14));
var_dump(unpack('f<', pack('g', 3.14))[1] === unpack('g', pack('g', 3.14))[1]);
var_dump(unpack('d>', pack('E', 3.14))[1] === unpack('E', pack('E', 3.14))[1]);
var_dump(unpack('f', pack('f', 1.5))[1] === 1.5);
var_dump(unpack('d', pack('d', 1.5))[1] === 1.5);
"#,
    );
    assert_eq!(
        out,
        "string(4) \"0201\"\nstring(4) \"0201\"\nstring(4) \"0102\"\nstring(4) \"0102\"\nbool(true)\nbool(true)\nstring(8) \"04030201\"\nstring(8) \"04030201\"\nstring(8) \"01020304\"\nstring(8) \"01020304\"\nbool(true)\nbool(true)\narray(1) {\n  [1]=>\n  int(258)\n}\narray(1) {\n  [1]=>\n  int(258)\n}\narray(1) {\n  [1]=>\n  int(-2)\n}\narray(1) {\n  [1]=>\n  int(-2)\n}\narray(1) {\n  [1]=>\n  int(-2)\n}\narray(1) {\n  [1]=>\n  int(-2)\n}\nstring(8) \"02010403\"\narray(2) {\n  [1]=>\n  int(258)\n  [2]=>\n  int(772)\n}\narray(2) {\n  [\"value\"]=>\n  int(258)\n  [\"count\"]=>\n  int(5)\n}\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n"
    );
}

/// php-src `pack_endian_modifiers_64.phpt`.
#[test]
fn test_pack_endian_modifiers_64_php86() {
    let out = run86(
        r#"<?php
var_dump(bin2hex(pack('q<', 0x0102030405060708)));
var_dump(bin2hex(pack('Q<', 0x0102030405060708)));
var_dump(bin2hex(pack('q>', 0x0102030405060708)));
var_dump(bin2hex(pack('Q>', 0x0102030405060708)));
var_dump(pack('q<', 0x0102030405060708) === pack('P', 0x0102030405060708));
var_dump(pack('q>', 0x0102030405060708) === pack('J', 0x0102030405060708));
var_dump(unpack('q<', "\x08\x07\x06\x05\x04\x03\x02\x01"));
var_dump(unpack('q>', "\x01\x02\x03\x04\x05\x06\x07\x08"));
var_dump(unpack('q<', "\xfe\xff\xff\xff\xff\xff\xff\xff"));
var_dump(unpack('q>', "\xff\xff\xff\xff\xff\xff\xff\xfe"));
"#,
    );
    assert_eq!(
        out,
        "string(16) \"0807060504030201\"\nstring(16) \"0807060504030201\"\nstring(16) \"0102030405060708\"\nstring(16) \"0102030405060708\"\nbool(true)\nbool(true)\narray(1) {\n  [1]=>\n  int(72623859790382856)\n}\narray(1) {\n  [1]=>\n  int(72623859790382856)\n}\narray(1) {\n  [1]=>\n  int(-2)\n}\narray(1) {\n  [1]=>\n  int(-2)\n}\n"
    );
}

/// php-src `pack_endian_modifiers_error.phpt`.
#[test]
fn test_pack_endian_modifiers_error_php86() {
    let out = run86(
        r#"<?php
$inherent = ['n<', 'v>', 'N<', 'V>', 'J<', 'P>', 'g<', 'G>', 'e<', 'E>'];
foreach ($inherent as $fmt) {
    try { pack($fmt, 1); }
    catch (ValueError $e) { echo 'pack(', $fmt, '): ', $e->getMessage(), "\n"; }
}
$unsupported = ['c<', 'C>', 'a<', 'A>', 'h<', 'H>', 'i<', 'I>', 'x<', 'X>', '@<'];
foreach ($unsupported as $fmt) {
    try { pack($fmt, 1); }
    catch (ValueError $e) { echo 'pack(', $fmt, '): ', $e->getMessage(), "\n"; }
}
foreach ($inherent as $fmt) {
    try { unpack($fmt, str_repeat("\0", 8)); }
    catch (ValueError $e) { echo 'unpack(', $fmt, '): ', $e->getMessage(), "\n"; }
}
foreach ($unsupported as $fmt) {
    try { unpack($fmt, str_repeat("\0", 8)); }
    catch (ValueError $e) { echo 'unpack(', $fmt, '): ', $e->getMessage(), "\n"; }
}
"#,
    );
    let mut expected = String::new();
    for fmt in ["n<", "v>", "N<", "V>", "J<", "P>", "g<", "G>", "e<", "E>"] {
        let modifier = &fmt[1..];
        let code = &fmt[..1];
        expected.push_str(&format!(
            "pack({fmt}): Endianness modifier '{modifier}' cannot be applied to format code '{code}' which already has inherent endianness\n"
        ));
    }
    for fmt in ["c<", "C>", "a<", "A>", "h<", "H>", "i<", "I>", "x<", "X>", "@<"] {
        let code = &fmt[..1];
        expected.push_str(&format!(
            "pack({fmt}): Endianness modifier is not supported for format code '{code}'\n"
        ));
    }
    for fmt in ["n<", "v>", "N<", "V>", "J<", "P>", "g<", "G>", "e<", "E>"] {
        let modifier = &fmt[1..];
        let code = &fmt[..1];
        expected.push_str(&format!(
            "unpack({fmt}): Endianness modifier '{modifier}' cannot be applied to format code '{code}' which already has inherent endianness\n"
        ));
    }
    for fmt in ["c<", "C>", "a<", "A>", "h<", "H>", "i<", "I>", "x<", "X>", "@<"] {
        let code = &fmt[..1];
        expected.push_str(&format!(
            "unpack({fmt}): Endianness modifier is not supported for format code '{code}'\n"
        ));
    }
    assert_eq!(out, expected);
}

/// php-src `unpack_endian_modifier_names.phpt`.
#[test]
fn test_unpack_endian_modifier_names_php86() {
    let out = run86(
        r#"<?php
var_dump(unpack('s<value', "\x02\x01"));
var_dump(unpack('s>value', "\x01\x02"));
var_dump(unpack('s<2name', "\x02\x01\x04\x03"));
var_dump(unpack('s<*name', "\x02\x01\x04\x03"));
var_dump(unpack('v1<value', "\x02\x01"));
var_dump(unpack('n2>name', "\x01\x02\x03\x04"));
var_dump(unpack('C1>name', "\x01"));
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [\"value\"]=>\n  int(258)\n}\narray(1) {\n  [\"value\"]=>\n  int(258)\n}\narray(2) {\n  [\"name1\"]=>\n  int(258)\n  [\"name2\"]=>\n  int(772)\n}\narray(2) {\n  [\"name1\"]=>\n  int(258)\n  [\"name2\"]=>\n  int(772)\n}\narray(1) {\n  [\"<value\"]=>\n  int(258)\n}\narray(2) {\n  [\">name1\"]=>\n  int(258)\n  [\">name2\"]=>\n  int(772)\n}\narray(1) {\n  [\">name\"]=>\n  int(1)\n}\n"
    );
}

/// php-src `unpack_endian_modifier_names_error.phpt`.
#[test]
fn test_unpack_endian_modifier_names_error_php86() {
    let out = run86(
        r#"<?php
foreach (["C>name", "@>name", "n<name", "V>name"] as $fmt) {
    try { unpack($fmt, "\x01\x02\x03\x04\x05\x06\x07\x08"); }
    catch (ValueError $e) { echo 'unpack(', $fmt, '): ', $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        "unpack(C>name): Endianness modifier is not supported for format code 'C'\nunpack(@>name): Endianness modifier is not supported for format code '@'\nunpack(n<name): Endianness modifier '<' cannot be applied to format code 'n' which already has inherent endianness\nunpack(V>name): Endianness modifier '>' cannot be applied to format code 'V' which already has inherent endianness\n"
    );
}

/// php-src `pack_float.phpt`, representative double/float cases.
#[test]
fn test_pack_float_php86() {
    let out = run86(
        r#"<?php
foreach (['e', 'E', 'g', 'G'] as $code) {
    echo $code, ':', bin2hex(pack($code, 3.14)), ':', bin2hex(pack($code, -0.591234709823149)), "\n";
}
var_dump(unpack('e', hex2bin('4a6ade0d65ebe23f')));
var_dump(unpack('E', hex2bin('3fe2eb650dde6a4a')));
var_dump(unpack('g', hex2bin('285b173f')));
var_dump(unpack('G', hex2bin('3f175b28')));
"#,
    );
    assert_eq!(
        out,
        "e:1f85eb51b81e0940:4a6ade0d65ebe2bf\nE:40091eb851eb851f:bfe2eb650dde6a4a\ng:c3f54840:285b17bf\nG:4048f5c3:bf175b28\narray(1) {\n  [1]=>\n  float(0.591234709823149)\n}\narray(1) {\n  [1]=>\n  float(0.591234709823149)\n}\narray(1) {\n  [1]=>\n  float(0.5912346839904785)\n}\narray(1) {\n  [1]=>\n  float(0.5912346839904785)\n}\n"
    );
}

/// php-src `pack_A.phpt`, `pack_Z.phpt`, and `pack_arrays.phpt` byte-level cases.
#[test]
fn test_pack_string_and_array_families_php86() {
    let out = run86(
        r#"<?php
echo bin2hex(pack('A5', 'foo ')), ':', bin2hex(pack('A4', 'fooo')), ':', bin2hex(pack('A4', 'foo')), "\n";
var_dump(unpack('A*', "foo\0\rbar\0 \t\r\n"));
var_dump(unpack('A4', "foo\0\rbar\0 \t\r\n"));
echo bin2hex(pack('Z0', 'f')), ':', bin2hex(pack('Z5', "foo\0")), ':', bin2hex(pack('Z4', 'fooo')), ':', bin2hex(pack('Z4', 'foo')), ':', bin2hex(pack('Z*', 'foo')), "\n";
var_dump(unpack('Z*', "foo\0\rbar\0 \t\r\n"));
var_dump(unpack('Z9', "foo\0\rbar\0 \t\r\n"));
var_dump(unpack('Z2', "\0\0"));
var_dump(unpack('Z2', "A\0"));
var_dump(unpack('Z2', 'AB'));
$str = pack('VVV', 0x00010203, 0x04050607, 0x08090a0b);
print_r(unpack('Vaa/Vbb/Vcc', $str));
print_r(unpack('V2aa/Vcc', $str));
print_r(unpack('V3aa', $str));
print_r(unpack('V*aa', $str));
print_r(unpack('V*', $str));
"#,
    );
    assert_eq!(
        out,
        "666f6f2020:666f6f6f:666f6f20\narray(1) {\n  [1]=>\n  string(8) \"foo\0\rbar\"\n}\narray(1) {\n  [1]=>\n  string(3) \"foo\"\n}\n:666f6f0000:666f6f00:666f6f00:666f6f00\narray(1) {\n  [1]=>\n  string(3) \"foo\"\n}\narray(1) {\n  [1]=>\n  string(3) \"foo\"\n}\narray(1) {\n  [1]=>\n  string(0) \"\"\n}\narray(1) {\n  [1]=>\n  string(1) \"A\"\n}\narray(1) {\n  [1]=>\n  string(2) \"AB\"\n}\nArray\n(\n    [aa] => 66051\n    [bb] => 67438087\n    [cc] => 134810123\n)\nArray\n(\n    [aa1] => 66051\n    [aa2] => 67438087\n    [cc] => 134810123\n)\nArray\n(\n    [aa1] => 66051\n    [aa2] => 67438087\n    [aa3] => 134810123\n)\nArray\n(\n    [aa1] => 66051\n    [aa2] => 67438087\n    [aa3] => 134810123\n)\nArray\n(\n    [1] => 66051\n    [2] => 67438087\n    [3] => 134810123\n)\n"
    );
}

/// php-src `unpack_offset.phpt` and `unpack_error.phpt`.
#[test]
fn test_unpack_offset_and_error_php86() {
    let out = run86(
        r#"<?php
$data = 'pad' . pack('ll', 0x01020304, 0x05060708);
$a = unpack('l2', $data, 3);
printf("0x%08x 0x%08x\n", $a[1], $a[2]);
printf("0x%08x 0x%08x\n", unpack('l', $data, 3)[1], unpack('@4/l', $data, 3)[1]);
try { unpack('l', 'foo', 10); } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { unpack('l', 'foo', -1); } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
try { unpack('B', pack('l', 65534)); } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "0x01020304 0x05060708\n0x01020304 0x05060708\nValueError: unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)\nValueError: unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)\nValueError: Invalid format type B\n"
    );
}

/// On PHP 8.5 the endianness modifier is not syntax; `<` remains an unknown format code.
#[test]
fn test_pack_endianness_is_php86_only() {
    let out = run85(
        r#"<?php
try { pack('s<', 1); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
var_dump(unpack('s<', "\x01\x02"));
"#,
    );
    assert_eq!(out, "Type <: unknown format code\narray(1) {\n  [\"<\"]=>\n  int(513)\n}\n");
}

/// Base pack/unpack behavior on the default PHP 8.5 profile, including case-insensitive calls.
#[test]
fn test_pack_base_family_php85() {
    let out = run85(
        r#"<?php
var_dump(bin2hex(pack('N', 0x12345678)));
var_dump(bin2hex(PACK('V', 0x12345678)));
var_dump(bin2hex(pack('C*', 1, 2, 3)));
var_dump(bin2hex(pack('a3', 'a')), bin2hex(pack('A3', 'a')));
var_dump(unpack('N', "\x12\x34\x56\x78")[1]);
var_dump(Unpack('C*', "\x01\x02\x03"));
var_dump(unpack('a3', 'abc'));
var_dump(bin2hex(pack('e', 1.0)));
"#,
    );
    assert_eq!(
        out,
        "string(8) \"12345678\"\nstring(8) \"78563412\"\nstring(6) \"010203\"\nstring(6) \"610000\"\nstring(6) \"612020\"\nint(305419896)\narray(3) {\n  [1]=>\n  int(1)\n  [2]=>\n  int(2)\n  [3]=>\n  int(3)\n}\narray(1) {\n  [1]=>\n  string(3) \"abc\"\n}\nstring(16) \"000000000000f03f\"\n"
    );
}
