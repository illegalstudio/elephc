---
title: "getimagesize()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 451
---

## getimagesize()

```php
function getimagesize(string $filename): mixed
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

For how `getimagesize` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/getimagesize.md).
