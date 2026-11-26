---
title: "filter_var()"
description: "Filters a variable with the given filter and returns the filtered value."
---

## filter_var()

```php
function filter_var(mixed $value, int $filter = FILTER_DEFAULT, array|int $options = 0): mixed
```

Filters a variable with the given filter and returns the filtered value.

**Parameters**:
- `$value` (`mixed`)
- `$filter` (`int`), default `FILTER_DEFAULT`, optional
- `$options` (`array|int`), default `0`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filter_var` is implemented in the compiler, see [the internals page](../../../internals/builtins/filter/filter_var.md).
