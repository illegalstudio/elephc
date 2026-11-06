---
title: "strtotime()"
description: "Parses an English textual datetime description into a Unix timestamp."
---

## strtotime()

```php
function strtotime(string $datetime, ?int $baseTimestamp = null): mixed
```

Parses an English textual datetime description into a Unix timestamp.

**Parameters**:
- `$datetime` (`string`)
- `$baseTimestamp` (`?int`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strtotime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/strtotime.md).
