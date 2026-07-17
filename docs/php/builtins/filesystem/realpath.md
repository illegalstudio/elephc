---
title: "realpath()"
description: "Returns canonicalized absolute pathname."
sidebar:
  order: 143
---

## realpath()

```php
function realpath(string $path): mixed
```

Returns canonicalized absolute pathname.

**Parameters**:
- `$path` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/realpath.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/realpath.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `realpath` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/realpath.md).

