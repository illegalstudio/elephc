---
title: "cairo_set_source_rgb()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 439
---

## cairo_set_source_rgb()

```php
function cairo_set_source_rgb(mixed $context, float $red, float $green, float $blue): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$red` (`float`)
- `$green` (`float`)
- `$blue` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_source_rgb` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_source_rgb.md).
