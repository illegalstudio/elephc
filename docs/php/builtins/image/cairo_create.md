---
title: "cairo_create()"
description: "Creates a drawing context for a surface."
sidebar:
  order: 404
---

## cairo_create()

```php
function cairo_create(mixed $surface): mixed
```

Creates a drawing context for a surface.

**Parameters**:
- `$surface` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_create.md).
