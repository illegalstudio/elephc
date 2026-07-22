---
title: "mktime()"
description: "Returns the Unix timestamp for a date."
sidebar:
  order: 101
---

## mktime()

```php
function mktime(int $hour, int $minute, int $second, int $month, int $day, int $year): int
```

Returns the Unix timestamp for a date.

**Parameters**:
- `$hour` (`int`)
- `$minute` (`int`)
- `$second` (`int`)
- `$month` (`int`)
- `$day` (`int`)
- `$year` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/mktime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/mktime.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `mktime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/mktime.md).
