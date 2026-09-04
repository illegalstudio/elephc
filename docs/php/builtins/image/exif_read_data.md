---
title: "exif_read_data()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 447
---

## exif_read_data()

```php
function exif_read_data(string $filename, string $required_sections = null, bool $as_arrays = false, bool $read_thumbnail = false): mixed
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

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `exif_read_data` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/exif_read_data.md).
