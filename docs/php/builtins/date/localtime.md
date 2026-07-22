---
title: "localtime()"
description: "Returns the local time."
sidebar:
  order: 99
---

## localtime()

```php
function localtime(int $timestamp = -1, bool $associative = false): array
```

Returns the local time.

**Parameters**:
- `$timestamp` (`int`), default `-1`, optional
- `$associative` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/localtime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/localtime.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `localtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/localtime.md).
