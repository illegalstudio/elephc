---
title: "cairo_set_source()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 438
---

## cairo_set_source()

```php
function cairo_set_source(mixed $context, mixed $pattern): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$pattern` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_source` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_source.md).
