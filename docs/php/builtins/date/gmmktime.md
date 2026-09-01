---
title: "gmmktime()"
description: "Returns the Unix timestamp for a GMT date."
sidebar:
  order: 105
---

## gmmktime()

```php
function gmmktime(int $hour, int $minute = null, int $second = null, int $month = null, int $day = null, int $year = null): int
```

Returns the Unix timestamp for a GMT date.

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
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/gmmktime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/gmmktime.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmmktime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gmmktime.md).
