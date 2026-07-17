---
title: "unlink()"
description: "Deletes a file."
sidebar:
  order: 156
---

## unlink()

```php
function unlink(string $filename): bool
```

Deletes a file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/unlink.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/unlink.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `unlink` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/unlink.md).

