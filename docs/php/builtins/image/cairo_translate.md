---
title: "cairo_translate()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 445
---

## cairo_translate()

```php
function cairo_translate(mixed $context, float $tx, float $ty): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$tx` (`float`)
- `$ty` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_translate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_translate.md).
