---
title: "cairo_set_source_rgba()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 440
---

## cairo_set_source_rgba()

```php
function cairo_set_source_rgba(mixed $context, float $red, float $green, float $blue, float $alpha): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$red` (`float`)
- `$green` (`float`)
- `$blue` (`float`)
- `$alpha` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_source_rgba` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_source_rgba.md).
