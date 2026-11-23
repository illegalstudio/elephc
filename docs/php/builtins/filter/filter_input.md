---
title: "filter_input()"
description: "Returns the filtered request input for the named variable, or null when it is absent."
---

## filter_input()

```php
function filter_input(int $type, string $var_name, int $filter = FILTER_DEFAULT, array|int $options = 0): mixed
```

Returns the filtered request input for the named variable, or null when it is absent.

**Parameters**:
- `$type` (`int`)
- `$var_name` (`string`)
- `$filter` (`int`), default `FILTER_DEFAULT`, optional
- `$options` (`array|int`), default `0`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filter_input` is implemented in the compiler, see [the internals page](../../../internals/builtins/filter/filter_input.md).
