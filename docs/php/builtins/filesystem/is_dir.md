---
title: "is_dir()"
description: "Tells whether the filename is a directory."
sidebar:
  order: 126
---

## is_dir()

```php
function is_dir(string $filename): bool
```

Tells whether the filename is a directory.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/is_dir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_dir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_dir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_dir.md).

