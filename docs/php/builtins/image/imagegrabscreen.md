---
title: "imagegrabscreen()"
description: "Captures the Windows desktop into a GdImage."
sidebar:
  order: 517
---

## imagegrabscreen()

```php
function imagegrabscreen(): mixed
```

Captures the Windows desktop into a GdImage.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported on Windows x86_64; the function is absent on non-Windows targets, matching php-src's `PHP_WIN32` guard.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagegrabscreen` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagegrabscreen.md).
