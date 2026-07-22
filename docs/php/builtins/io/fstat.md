---
title: "fstat()"
description: "Gets information about a file using an open file pointer."
sidebar:
  order: 178
---

## fstat()

```php
function fstat(resource $stream): mixed
```

Gets information about a file using an open file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fstat.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fstat.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fstat` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fstat.md).
