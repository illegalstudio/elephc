---
title: "fpassthru()"
description: "Output all remaining data on a file pointer."
sidebar:
  order: 170
---

## fpassthru()

```php
function fpassthru(resource $stream): int
```

Output all remaining data on a file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fpassthru.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fpassthru.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fpassthru` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fpassthru.md).

