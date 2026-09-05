---
title: "timezone_abbreviations_list()"
description: "Implemented by the compiler-injected tz prelude."
sidebar:
  order: 243
---

## timezone_abbreviations_list()

```php
function timezone_abbreviations_list(): mixed
```

Implemented by the compiler-injected tz prelude.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected tz prelude.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_abbreviations_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_abbreviations_list.md).
