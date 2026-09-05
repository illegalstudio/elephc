---
title: "cairo_transform()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 444
---

## cairo_transform()

```php
function cairo_transform(mixed $context, mixed $matrix): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$matrix` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_transform` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_transform.md).
