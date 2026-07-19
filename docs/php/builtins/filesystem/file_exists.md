---
title: "file_exists()"
description: "Checks whether a file or directory exists."
sidebar:
  order: 112
---

## file_exists()

```php
function file_exists(string $filename): bool
```

Checks whether a file or directory exists.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/file_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/file_exists.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `file_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/file_exists.md).

