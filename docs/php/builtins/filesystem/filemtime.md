---
title: "filemtime()"
description: "Gets file modification time."
sidebar:
  order: 119
---

## filemtime()

```php
function filemtime(string $filename): int
```

Gets file modification time.

**Parameters**:
- `$filename` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/filemtime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filemtime.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `filemtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/filemtime.md).
