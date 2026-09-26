---
title: "timezone_abbreviations_list()"
description: "Returns every timezone abbreviation with its offset and DST flag."
sidebar:
  order: 246
---

## timezone_abbreviations_list()

```php
function timezone_abbreviations_list(): mixed
```

Returns every timezone abbreviation with its offset and DST flag.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected tz prelude.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_abbreviations_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_abbreviations_list.md).
