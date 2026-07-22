---
title: "fileowner()"
description: "Gets file owner."
sidebar:
  order: 120
---

## fileowner()

```php
function fileowner(string $filename): mixed
```

Gets file owner.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fileowner.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fileowner.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fileowner` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fileowner.md).
