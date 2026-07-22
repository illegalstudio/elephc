---
title: "is_executable()"
description: "Tells whether the filename is executable."
sidebar:
  order: 129
---

## is_executable()

```php
function is_executable(string $filename): bool
```

Tells whether the filename is executable.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/is_executable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_executable.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_executable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_executable.md).
