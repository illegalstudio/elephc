---
title: "filter_id()"
description: "Returns the filter id for the given filter name, or false when the name is unknown."
---

## filter_id()

```php
function filter_id(string $name): int|false
```

Returns the filter id for the given filter name, or false when the name is unknown.

**Parameters**:
- `$name` (`string`)

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filter_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/filter/filter_id.md).
