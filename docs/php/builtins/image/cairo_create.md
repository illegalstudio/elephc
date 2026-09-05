---
title: "cairo_create()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 401
---

## cairo_create()

```php
function cairo_create(mixed $surface): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$surface` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_create.md).
