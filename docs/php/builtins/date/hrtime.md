---
title: "hrtime()"
description: "Returns the current high-resolution time."
sidebar:
  order: 96
---

## hrtime()

```php
function hrtime(bool $as_number = false): mixed
```

Returns the current high-resolution time.

**Parameters**:
- `$as_number` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/hrtime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/hrtime.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hrtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/hrtime.md).

