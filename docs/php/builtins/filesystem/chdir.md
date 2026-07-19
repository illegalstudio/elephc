---
title: "chdir()"
description: "Changes the current directory."
sidebar:
  order: 103
---

## chdir()

```php
function chdir(string $directory): bool
```

Changes the current directory.

**Parameters**:
- `$directory` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/chdir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/chdir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `chdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chdir.md).

