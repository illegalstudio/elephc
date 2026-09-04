---
title: "imagegif()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 511
---

## imagegif()

```php
function imagegif(mixed $image, string $file = null): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$file` (`string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagegif` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagegif.md).
