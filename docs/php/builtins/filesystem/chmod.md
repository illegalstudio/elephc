---
title: "chmod()"
description: "Changes file mode."
sidebar:
  order: 114
---

## chmod()

```php
function chmod(string $filename, int $permissions): bool
```

Changes file mode.

**Parameters**:
- `$filename` (`string`)
- `$permissions` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/chmod.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/chmod.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `chmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chmod.md).
