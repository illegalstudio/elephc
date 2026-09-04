---
title: "cairo_set_fill_rule()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 433
---

## cairo_set_fill_rule()

```php
function cairo_set_fill_rule(mixed $context, int $fillRule): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$fillRule` (`int`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_fill_rule` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_fill_rule.md).
