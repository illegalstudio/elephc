---
title: "rewinddir()"
description: "Rewind directory handle."
sidebar:
  order: 205
---

## rewinddir()

```php
function rewinddir(resource $dir_handle): void
```

Rewind directory handle.

**Parameters**:
- `$dir_handle` (`resource`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/rewinddir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/rewinddir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rewinddir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/rewinddir.md).

