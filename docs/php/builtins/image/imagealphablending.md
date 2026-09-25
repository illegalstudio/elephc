---
title: "imagealphablending()"
description: "Turns alpha blending on or off for subsequent drawing."
sidebar:
  order: 462
---

## imagealphablending()

```php
function imagealphablending(mixed $image, bool $enable): bool
```

Turns alpha blending on or off for subsequent drawing.

**Parameters**:
- `$image` (`mixed`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagealphablending` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagealphablending.md).
