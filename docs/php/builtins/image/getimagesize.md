---
title: "getimagesize()"
description: "Returns an image file's size, type, and MIME type."
sidebar:
  order: 454
---

## getimagesize()

```php
function getimagesize(string $filename): mixed
```

Returns an image file's size, type, and MIME type.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getimagesize` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/getimagesize.md).
