---
title: "fseek()"
description: "Seeks on a file pointer."
sidebar:
  order: 175
---

## fseek()

```php
function fseek(resource $stream, int $offset, int $whence = 0): int
```

Seeks on a file pointer.

**Parameters**:
- `$stream` (`resource`)
- `$offset` (`int`)
- `$whence` (`int`), default `0`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fseek.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fseek.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fseek` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fseek.md).

