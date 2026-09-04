---
title: "imagesy()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 533
---

## imagesy()

```php
function imagesy(mixed $image): int
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesy` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesy.md).
