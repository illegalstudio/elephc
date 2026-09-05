---
title: "ftell()"
description: "Returns the current position of the file read/write pointer."
sidebar:
  order: 187
---

## ftell()

```php
function ftell(resource $stream): int
```

Returns the current position of the file read/write pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/ftell.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/ftell.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ftell` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ftell.md).
