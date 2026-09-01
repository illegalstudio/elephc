---
title: "mktime()"
description: "Returns the Unix timestamp for a date."
sidebar:
  order: 109
---

## mktime()

```php
function mktime(int $hour, int $minute = null, int $second = null, int $month = null, int $day = null, int $year = null): int
```

Returns the Unix timestamp for a date.

**Parameters**:
- `$hour` (`int`)
- `$minute` (`int`), default `null`, optional
- `$second` (`int`), default `null`, optional
- `$month` (`int`), default `null`, optional
- `$day` (`int`), default `null`, optional
- `$year` (`int`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/mktime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/mktime.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mktime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/mktime.md).
