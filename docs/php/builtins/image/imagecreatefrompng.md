---
title: "imagecreatefrompng()"
description: "Creates an image from a PNG file."
sidebar:
  order: 494
---

## imagecreatefrompng()

```php
function imagecreatefrompng(string $filename): mixed
```

Creates an image from a PNG file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatefrompng` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefrompng.md).
