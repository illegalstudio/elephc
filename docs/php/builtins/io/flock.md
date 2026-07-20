---
title: "flock()"
description: "Portable advisory file locking."
sidebar:
  order: 168
---

## flock()

```php
function flock(resource $stream, int $operation, bool $would_block = null): bool
```

Portable advisory file locking.

**Parameters**:
- `$stream` (`resource`)
- `$operation` (`int`)
- `$would_block` (`bool`), passed by reference, default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/flock.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/flock.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `flock` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/flock.md).

