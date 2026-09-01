---
title: "filesize()"
description: "Gets file size."
sidebar:
  order: 130
---

## filesize()

```php
function filesize(string $filename): mixed
```

Gets file size.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/filesize.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filesize.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filesize` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/filesize.md).
