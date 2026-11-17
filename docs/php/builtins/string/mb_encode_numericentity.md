---
title: "mb_encode_numericentity()"
description: "Encodes mapped characters as decimal or hexadecimal numeric entities."
---

## mb_encode_numericentity()

```php
function mb_encode_numericentity(string $string, array $map, ?string $encoding = null, bool $hex = false): string
```

Encodes mapped characters as decimal or hexadecimal numeric entities.

**Parameters**:
- `$string` (`string`)
- `$map` (`array`)
- `$encoding` (`?string`), default `null`, optional
- `$hex` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_encode_numericentity` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_encode_numericentity.md).
