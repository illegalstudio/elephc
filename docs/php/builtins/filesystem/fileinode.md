---
title: "fileinode()"
description: "Gets file inode."
sidebar:
  order: 116
---

## fileinode()

```php
function fileinode(string $filename): mixed
```

Gets file inode.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fileinode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fileinode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fileinode` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fileinode.md).

