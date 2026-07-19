---
title: "date_default_timezone_set()"
description: "Sets the default timezone."
sidebar:
  order: 92
---

## date_default_timezone_set()

```php
function date_default_timezone_set(string $timezoneId): bool
```

Sets the default timezone.

**Parameters**:
- `$timezoneId` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/date_default_timezone_set.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/date_default_timezone_set.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `date_default_timezone_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_default_timezone_set.md).

