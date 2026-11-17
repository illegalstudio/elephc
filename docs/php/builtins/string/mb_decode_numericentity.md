---
title: "mb_decode_numericentity()"
description: "Decodes numeric entities selected by the ordered conversion map."
---

## mb_decode_numericentity()

```php
function mb_decode_numericentity(string $string, array $map, ?string $encoding = null): string
```

Decodes numeric entities selected by the ordered conversion map.

**Parameters**:
- `$string` (`string`)
- `$map` (`array`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_decode_numericentity` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_decode_numericentity.md).
