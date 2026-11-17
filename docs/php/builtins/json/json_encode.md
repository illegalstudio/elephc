---
title: "json_encode()"
description: "Returns the JSON representation of a value."
---

## json_encode()

```php
function json_encode(mixed $value, int $flags = 0, int $depth = 512): string
```

Returns the JSON representation of a value.

**Parameters**:
- `$value` (`mixed`)
- `$flags` (`int`), default `0`, optional
- `$depth` (`int`), default `512`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `json_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_encode.md).
