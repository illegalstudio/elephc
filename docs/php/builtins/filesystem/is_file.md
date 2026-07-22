---
title: "is_file()"
description: "Tells whether the filename is a regular file."
sidebar:
  order: 130
---

## is_file()

```php
function is_file(string $filename): bool
```

Tells whether the filename is a regular file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/is_file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_file.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_file.md).
