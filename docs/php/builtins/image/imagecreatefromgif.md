---
title: "imagecreatefromgif()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 487
---

## imagecreatefromgif()

```php
function imagecreatefromgif(string $filename): mixed
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

For how `imagecreatefromgif` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefromgif.md).
