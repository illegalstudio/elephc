---
title: "cairo_new_sub_path()"
description: "Begins a new subpath without a starting point."
sidebar:
  order: 423
---

## cairo_new_sub_path()

```php
function cairo_new_sub_path(mixed $context): void
```

Begins a new subpath without a starting point.

**Parameters**:
- `$context` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_new_sub_path` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_new_sub_path.md).
