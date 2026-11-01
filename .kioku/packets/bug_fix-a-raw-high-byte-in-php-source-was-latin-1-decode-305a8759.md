---
id: bug_fix-a-raw-high-byte-in-php-source-was-latin-1-decode-305a8759
type: bug_fix
title: "A raw high byte in PHP source was Latin-1 decoded, so every byte operation on it was wrong"
description: "Composer's classmap and symfony/cache's class \\xA9 put non-UTF-8 bytes in source; the decode mapped them through Latin-1 and strlen/ord/bin2hex all disagreed with php, while the escape spelling was already correct"
created: 2026-09-22
verified_by: "php 8.5.10 on raw-byte and escaped fixtures, cargo test --lib at the 29-failure baseline, Symfony --web 8/8"
sources:
  - path: src/source.rs
    blob: 7552a454cf2be453c87dba7394381038f8819c7a
    lines: 117-200
    snip: 3f86d33b2e84
    anchor: "pub(crate) fn read_physical_source(path: impl AsRef<Path>) -> std::io::Result<String> {"
  - path: src/lexer/literals/strings.rs
    blob: bb3427ee58b392942b03725e94e92a65b69dd1ea
    lines: 750-765
    snip: c2ecbebc3e68
    anchor: "crate::string_bytes::push_escaped_byte(byte, out);"
  - path: src/string_bytes.rs
    blob: b0ccb08060a362acaca8403e1c1269346d2c7c86
    lines: 1-55
    snip: 19e85c62592a
    anchor: "const ESCAPED_BYTE_BASE: u32 = 0xe000;"
---

# A raw high byte in PHP source was Latin-1 decoded, so every byte operation on it was wrong

## Fact

PHP strings and identifiers are BYTE strings; bytes `0x80..=0xff` are legal in both. Composer
surfaces this: `symfony/cache` declares `class \xA9` -- one non-ASCII byte, no namespace,
documented as "a short namespace-less class to serialize items with metadata" -- and
`vendor/composer/autoload_static.php` carries the same byte as a classmap key. That file is not
valid UTF-8, which also makes `ugrep` treat it as binary and return NOTHING for any pattern in it.

`source::decode_physical_source` mapped each malformed byte through LATIN-1, so `0xA9` became
`U+00A9`, which re-encodes to TWO bytes. MEASURED against php 8.5.10 on a raw `0xA9` byte written
into the source:

    strlen("<byte>")     php 1        elephc 2
    ord("<byte>")        php 169      elephc 194
    bin2hex("<byte>")    php a9       elephc c2a9
    bin2hex("a<byte>b")  php 61a962   elephc 61c2a962

The ESCAPE spelling `"\xA9"` was already correct, on all of those, because the lexer routes it
through `crate::string_bytes` -- a private-use marker range `U+E000..=U+E0FF` that survives to
`literal_bytes` at codegen. So the representation could already carry an arbitrary byte; only the
SOURCE DECODE was lossy.

FIX: a malformed source byte takes the same `string_bytes` path as its escaped spelling. Two
supporting pieces are not optional:
  - `decode_physical_source` normalises a source that SPELLS a marker-range char itself, or that
    char could not be told apart from a decoded byte. The guard is a byte scan for `0xEE` (every
    marker's UTF-8 lead byte) before any char walk, because this runs on every file read.
  - the lexer's `push_literal_char` wrapper passes a marker through instead of applying the
    general collision rule, which otherwise turned one raw byte into the three bytes of U+E0A9.

WHY IT WENT UNNOTICED: internal coherence was preserved. A raw-byte class name and a raw-byte
string literal were transformed alike, so they still matched each other; only bytes LEAVING the
program differed. `class_exists` on such a class answered true both before and after.

STILL WRONG, and measured: `get_class()` on a class declared with a raw byte returns THREE bytes
(the marker's UTF-8), where php returns one. Three emission sites were converted to
`string_bytes::literal_bytes` -- `context::intern_class_name_data`,
`member_queries::emit_branch_if_dynamic_class_like_exists_candidate`, and the `_class_name_<id>`
table in `runtime/data/user.rs` (both its `.quad` length and its `.ascii` bytes) -- and the name
still comes out as the marker, so at least one more path emits it. Asking for such a class by its
TRUE byte (`class_exists("\xA9")` written as an escape, or `chr(0xA9)`) fails, and failed before
this change too: it is not a regression, it is the identifier half of the same hole.

REGRESSION CHECK: `cargo test --release --lib -p elephc` keeps its exact 29-failure baseline
(2128 passed, +1 for a new collision test), and Symfony `--web` answers 8/8 prod routes
byte-identical. `runtime_cache_pruning_preserves_cross_process_prepared_object_lease` fails when
run ALONE while builds compete for the machine and passes in the full suite -- it spawns a child
process and waits on a lease, so judge it from a full run, never from an isolated one.

## Why

It is a silent miscompile for any binary data in a string literal, and the obvious Latin-1 reading looks reversible but is not
