---
title: "stream_resolve_include_path()"
description: "Resolves filename against the include path."
sidebar:
  order: 213
---

## stream_resolve_include_path()

```php
function stream_resolve_include_path(string $filename): mixed
```

Resolves filename against the include path.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_resolve_include_path.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_resolve_include_path.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_resolve_include_path` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_resolve_include_path.md).

