---
title: "base_convert()"
description: "Converts a number between two arbitrary bases from 2 to 36."
---

## base_convert()

```php
function base_convert(string $num, int $from_base, int $to_base): string
```

Converts a number between two arbitrary bases from 2 to 36.

**Parameters**:
- `$num` (`string`)
- `$from_base` (`int`)
- `$to_base` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `base_convert` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/base_convert.md).
