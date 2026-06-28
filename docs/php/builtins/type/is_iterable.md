---
title: "is_iterable()"
description: "Lowers `is_iterable()` for concrete values and boxed Mixed payloads."
sidebar:
  order: 403
---

## is_iterable()

```php
function is_iterable(mixed $value): bool
```

Lowers `is_iterable()` for concrete values and boxed Mixed payloads.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_iterable` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_iterable.md).

