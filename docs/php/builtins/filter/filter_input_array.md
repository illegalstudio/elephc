---
title: "filter_input_array()"
description: "Returns filtered request inputs, keyed by the variable names in the options definition."
---

## filter_input_array()

```php
function filter_input_array(int $type, array|int $options = FILTER_DEFAULT, bool $add_empty = true): array|false|null
```

Returns filtered request inputs, keyed by the variable names in the options definition.

**Parameters**:
- `$type` (`int`)
- `$options` (`array|int`), default `FILTER_DEFAULT`, optional
- `$add_empty` (`bool`), default `true`, optional

**Returns**: `array|false|null`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filter_input_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/filter/filter_input_array.md).
