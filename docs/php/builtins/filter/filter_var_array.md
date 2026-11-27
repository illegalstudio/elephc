---
title: "filter_var_array()"
description: "Filters several values from one array using a definition of filters and flags."
---

## filter_var_array()

```php
function filter_var_array(array $array, array|int $options = FILTER_DEFAULT, bool $add_empty = true): array|false|null
```

Filters several values from one array using a definition of filters and flags.

**Parameters**:
- `$array` (`array`)
- `$options` (`array|int`), default `FILTER_DEFAULT`, optional
- `$add_empty` (`bool`), default `true`, optional

**Returns**: `array|false|null`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filter_var_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/filter/filter_var_array.md).
