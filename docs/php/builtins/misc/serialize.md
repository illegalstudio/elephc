---
title: "serialize()"
description: "Generates a storable representation of a value."
---

## serialize()

```php
function serialize(mixed $value): string
```

Generates a storable representation of a value.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `serialize` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/serialize.md).
