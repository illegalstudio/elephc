---
title: "imagecreatefrombmp()"
description: "Creates an image from a BMP file."
sidebar:
  order: 491
---

## imagecreatefrombmp()

```php
function imagecreatefrombmp(string $filename): mixed
```

Creates an image from a BMP file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatefrombmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefrombmp.md).
