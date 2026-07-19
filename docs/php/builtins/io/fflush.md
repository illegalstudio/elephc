---
title: "fflush()"
description: "Flushes the output to a file."
sidebar:
  order: 161
---

## fflush()

```php
function fflush(resource $stream): bool
```

Flushes the output to a file.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fflush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fflush.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fflush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fflush.md).

