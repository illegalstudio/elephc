---
title: "cairo_rotate()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 430
---

## cairo_rotate()

```php
function cairo_rotate(mixed $context, float $angle): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$angle` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_rotate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_rotate.md).
