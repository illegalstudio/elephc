---
title: "strtr()"
description: "Translates bytes pairwise, or applies longest-match-first replacement pairs."
---

## strtr()

```php
function strtr(string $string, array|string $from, ?string $to = null): string
```

Translates bytes pairwise, or applies longest-match-first replacement pairs.

**Parameters**:
- `$string` (`string`)
- `$from` (`array|string`)
- `$to` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strtr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strtr.md).
