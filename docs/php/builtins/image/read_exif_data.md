---
title: "read_exif_data()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 539
---

## read_exif_data()

```php
function read_exif_data(string $filename, string $required_sections = null, bool $as_arrays = false, bool $read_thumbnail = false): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$filename` (`string`)
- `$required_sections` (`string`), default `null`, optional
- `$as_arrays` (`bool`), default `false`, optional
- `$read_thumbnail` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `read_exif_data` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/read_exif_data.md).
