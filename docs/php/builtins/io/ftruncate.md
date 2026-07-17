---
title: "ftruncate()"
description: "Truncates a file to a given length."
sidebar:
  order: 179
---

## ftruncate()

```php
function ftruncate(resource $stream, int $size): bool
```

Truncates a file to a given length.

**Parameters**:
- `$stream` (`resource`)
- `$size` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/ftruncate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/ftruncate.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ftruncate` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ftruncate.md).

