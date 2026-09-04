---
title: "timezone_identifiers_list()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 244
---

## timezone_identifiers_list()

```php
function timezone_identifiers_list(int $timezoneGroup = DateTimeZone::ALL, string $countryCode = null): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$timezoneGroup` (`int`), default `DateTimeZone::ALL`, optional
- `$countryCode` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_identifiers_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_identifiers_list.md).
