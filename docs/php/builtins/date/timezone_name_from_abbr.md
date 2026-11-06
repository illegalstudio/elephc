---
title: "timezone_name_from_abbr()"
description: "Resolves a timezone abbreviation and offset to an identifier."
---

## timezone_name_from_abbr()

```php
function timezone_name_from_abbr(string $abbr, int $utcOffset = -1, int $isDST = -1): mixed
```

Resolves a timezone abbreviation and offset to an identifier.

**Parameters**:
- `$abbr` (`string`)
- `$utcOffset` (`int`), default `-1`, optional
- `$isDST` (`int`), default `-1`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_name_from_abbr` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_name_from_abbr.md).
