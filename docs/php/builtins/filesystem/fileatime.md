---
title: "fileatime()"
description: "Gets last access time of file."
sidebar:
  order: 122
---

## fileatime()

```php
function fileatime(string $filename): mixed
```

Gets last access time of file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fileatime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fileatime.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fileatime` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fileatime.md).
