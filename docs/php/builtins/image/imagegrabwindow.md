---
title: "imagegrabwindow()"
description: "Captures a Windows HWND or its client area into a GdImage."
sidebar:
  order: 518
---

## imagegrabwindow()

```php
function imagegrabwindow(int $handle, bool $client_area = false): mixed
```

Captures a Windows HWND or its client area into a GdImage.

**Parameters**:
- `$handle` (`int`)
- `$client_area` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported on Windows x86_64; the function is absent on non-Windows targets, matching php-src's `PHP_WIN32` guard.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagegrabwindow` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagegrabwindow.md).
