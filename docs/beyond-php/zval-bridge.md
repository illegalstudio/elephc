---
title: "zval Bridge"
description: "Convert elephc values to and from PHP zval structs with zval_pack, zval_unpack, zval_type, and zval_free."
sidebar:
  order: 8
---

The zval bridge is a compiler extension that converts native elephc runtime
values into PHP-shaped `zval` structures and back. It is the foundation for
linking a compiled program against a real PHP extension shared library: the
extension's C API speaks in `zval*` arguments and return values, so elephc
must be able to hand values over in that exact memory layout and read values
back out.

These four builtins have no PHP equivalent — they are elephc-specific and exist
only for the extension bridge. They work on every supported target (macOS
aarch64, Linux aarch64, Linux x86_64).

## Builtins

| Builtin | Signature | Description |
|---|---|---|
| `zval_pack` | `zval_pack(mixed $value): ptr` | Allocate a 16-byte PHP `zval` and fill it from the elephc value. Returns an opaque pointer. |
| `zval_unpack` | `zval_unpack(ptr $zval): mixed` | Rebuild an elephc value from a PHP `zval`. The zval keeps ownership of its storage; the returned value is independent. |
| `zval_type` | `zval_type(ptr $zval): int` | Read the PHP `IS_*` type byte at `zval+8`. |
| `zval_free` | `zval_free(ptr $zval): void` | Release the `zval` storage and every owned child (strings, arrays, keys). |

`zval_pack` accepts any value (int, float, bool, string, null, indexed array,
associative array, nested combinations). The returned pointer is untyped `ptr`
and is only meaningful to the bridge builtins and to C code that understands the
`zval` layout.

## PHP type bytes

`zval_type` returns the PHP `IS_*` constant stored at `zval+8`:

| Byte | PHP type |
|---|---|
| 1 | `IS_NULL` |
| 2 | `IS_FALSE` |
| 3 | `IS_TRUE` |
| 4 | `IS_LONG` (integer) |
| 5 | `IS_DOUBLE` (float) |
| 6 | `IS_STRING` |
| 7 | `IS_ARRAY` |
| 8 | `IS_OBJECT` |
| 9 | `IS_RESOURCE` |

For refcounted kinds (string, array, object, resource) the byte carries
`IS_TYPE_REFCOUNTED` and the value slot holds a pointer to the corresponding
PHP-shaped heap object (`zend_string`, `zend_array`, etc.).

## Supported conversions

| elephc value | packs to | unpacks back to |
|---|---|---|
| `int` | `IS_LONG` | `int` |
| `float` | `IS_DOUBLE` | `float` |
| `true` / `false` | `IS_TRUE` / `IS_FALSE` | `bool` |
| `null` | `IS_NULL` | `null` |
| `string` | `IS_STRING` (`zend_string` copy) | `string` (owned copy) |
| indexed array (`[1, 2, 3]`) | `IS_ARRAY` (packed `HashTable`) | indexed array |
| assoc array (`["a" => 1]`) | `IS_ARRAY` (hash `HashTable`) | assoc array |
| nested arrays | `IS_ARRAY` (nested `HashTable`) | nested arrays |

Strings and arrays are **deep-copied** into freshly allocated PHP-shaped heap
objects, so the packed `zval` owns independent storage and the original elephc
value is untouched. `zval_free` releases that storage, including every nested
string, array value, and hash key.

## Hash table layout

Associative arrays pack into a real PHP `HashTable` so a linked extension sees
a structurally correct value:

- `nTableMask = -nTableSize` (a non-packed hash; packed/indexed arrays use
  `nTableMask = -2`, the `HT_MIN_MASK`).
- A hash index of `nTableSize` 32-bit slots, each initialized to
  `HT_INVALID_IDX` (`0xFFFFFFFF`).
- `nTableSize` 32-byte `Bucket`s, each holding the 16-byte value `zval`, the
  8-byte key hash `h`, and the 8-byte `key` pointer (a `zend_string` for string
  keys, `NULL` for integer keys).
- String-key hash values use DJBX33A (`hash = 5381; hash = hash * 33 + c` per
  byte, with the high bit set so the hash is never the zero sentinel), matching
  `zend_inline_hash_func`. The collision chain lives in `bucket.u2` (`Z_NEXT`),
  threaded through `arHash[h | nTableMask]`.

`zval_unpack` reverses both layouts: packed `HashTable`s rebuild as elephc
indexed arrays and hash `HashTable`s rebuild as elephc assoc arrays, reading
string keys from each bucket's `zend_string` and integer keys from `bucket.h`.

## Example

```php
<?php
$z = zval_pack(["a" => 1, "b" => 2]);
echo zval_type($z);   // 7  (IS_ARRAY)
zval_free($z);
```

A larger end-to-end example lives at `examples/zval-pack/main.php`.

## Ownership and lifetimes

- `zval_pack` returns a pointer the caller owns. Pass it to `zval_free` when
  done, or it leaks (the bridge never tracks these allocations for you).
- `zval_unpack` does **not** consume the `zval`; the original pointer stays
  valid and still needs `zval_free`. The returned elephc value is an independent
  copy (strings are re-persisted, arrays are rebuilt).
- `zval_free` is null-safe and recurses through arrays, freeing each
  `zend_string` key and every owned child value. Freeing a `zval` produced by
  `zval_pack` exactly once is the correct lifecycle; double-free or use-after-
  `zval_free` of the same pointer is undefined.
- The opaque `ptr` returned by `zval_pack` is untyped to the type checker. Do
  not dereference it with `ptr_get` — only the bridge builtins understand the
  `zval` layout.