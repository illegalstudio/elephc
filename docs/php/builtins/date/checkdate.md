---
title: "checkdate()"
description: "Validates a Gregorian date."
sidebar:
  order: 91
---

## checkdate()

```php
function checkdate(int $month, int $day, int $year): bool
```

Validates a Gregorian date.

**Parameters**:
- `$month` (`int`)
- `$day` (`int`)
- `$year` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/checkdate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/checkdate.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `checkdate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/checkdate.md).
