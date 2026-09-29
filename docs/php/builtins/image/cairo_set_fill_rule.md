---
title: "cairo_set_fill_rule()"
description: "Selects the fill rule used to decide which regions a path encloses."
sidebar:
  order: 436
---

## cairo_set_fill_rule()

```php
function cairo_set_fill_rule(mixed $context, int $fillRule): void
```

Selects the fill rule used to decide which regions a path encloses.

**Parameters**:
- `$context` (`mixed`)
- `$fillRule` (`int`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_fill_rule` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_fill_rule.md).
