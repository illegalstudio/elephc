---
title: "time()"
description: "Returns the current Unix timestamp."
sidebar:
  order: 103
---

## time()

```php
function time(): int
```

Returns the current Unix timestamp.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/time.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/time.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `time` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/time.md).
