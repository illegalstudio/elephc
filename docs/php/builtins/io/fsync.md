---
title: "fsync()"
description: "Synchronizes changes to the file (including meta-data)."
sidebar:
  order: 179
---

## fsync()

```php
function fsync(resource $stream): bool
```

Synchronizes changes to the file (including meta-data).

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fsync.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fsync.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fsync` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fsync.md).
