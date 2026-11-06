---
title: "date_modify()"
description: "Applies a relative modifier such as \"+1 day\" to a DateTime."
---

## date_modify()

```php
function date_modify(mixed $object, string $modifier): mixed
```

Applies a relative modifier such as "+1 day" to a DateTime.

**Parameters**:
- `$object` (`mixed`)
- `$modifier` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_modify` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_modify.md).
