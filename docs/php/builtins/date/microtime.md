---
title: "microtime()"
description: "Returns the current Unix timestamp with microseconds."
sidebar:
  order: 100
---

## microtime()

```php
function microtime(bool $as_float = false): mixed
```

Returns the current Unix timestamp with microseconds.

**Parameters**:
- `$as_float` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/microtime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/microtime.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `microtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/microtime.md).
