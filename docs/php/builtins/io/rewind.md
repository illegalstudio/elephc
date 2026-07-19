---
title: "rewind()"
description: "Rewind the position of a file pointer."
sidebar:
  order: 191
---

## rewind()

```php
function rewind(resource $stream): bool
```

Rewind the position of a file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/rewind.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/rewind.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rewind` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/rewind.md).

