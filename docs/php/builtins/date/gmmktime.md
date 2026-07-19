---
title: "gmmktime()"
description: "Returns the Unix timestamp for a GMT date."
sidebar:
  order: 95
---

## gmmktime()

```php
function gmmktime(int $hour, int $minute, int $second, int $month, int $day, int $year): int
```

Returns the Unix timestamp for a GMT date.

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
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/gmmktime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/gmmktime.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gmmktime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gmmktime.md).

