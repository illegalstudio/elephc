---
title: "exif_imagetype()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 446
---

## exif_imagetype()

```php
function exif_imagetype(string $filename): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `exif_imagetype` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/exif_imagetype.md).
