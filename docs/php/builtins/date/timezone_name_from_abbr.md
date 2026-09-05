---
title: "timezone_name_from_abbr()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 246
---

## timezone_name_from_abbr()

```php
function timezone_name_from_abbr(string $abbr, int $utcOffset = -1, int $isDST = -1): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$abbr` (`string`)
- `$utcOffset` (`int`), default `-1`, optional
- `$isDST` (`int`), default `-1`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_name_from_abbr` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_name_from_abbr.md).
