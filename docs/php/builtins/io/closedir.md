---
title: "closedir()"
description: "Closes directory handle."
sidebar:
  order: 166
---

## closedir()

```php
function closedir(resource $dir_handle = null): void
```

Closes directory handle.

**Parameters**:
- `$dir_handle` (`resource`), default `null`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/closedir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/closedir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `closedir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/closedir.md).
