---
title: "cairo_close_path()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 400
---

## cairo_close_path()

```php
function cairo_close_path(mixed $context): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_close_path` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_close_path.md).
