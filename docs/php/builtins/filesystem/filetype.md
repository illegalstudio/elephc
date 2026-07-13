---
title: "filetype()"
description: "Gets file type."
sidebar:
  order: 121
---

## filetype()

```php
function filetype(string $filename): mixed
```

Gets file type.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/filetype.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filetype.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `filetype` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/filetype.md).

