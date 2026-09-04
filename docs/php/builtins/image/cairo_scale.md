---
title: "cairo_scale()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 432
---

## cairo_scale()

```php
function cairo_scale(mixed $context, float $sx, float $sy): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$sx` (`float`)
- `$sy` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_scale` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_scale.md).
