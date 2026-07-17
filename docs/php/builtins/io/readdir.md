---
title: "readdir()"
description: "Read entry from directory handle."
sidebar:
  order: 190
---

## readdir()

```php
function readdir(resource $dir_handle): mixed
```

Read entry from directory handle.

**Parameters**:
- `$dir_handle` (`resource`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/readdir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/readdir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `readdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/readdir.md).

