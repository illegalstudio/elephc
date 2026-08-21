---
title: "mkdir()"
description: "Makes a directory."
sidebar:
  order: 147
---

## mkdir()

```php
function mkdir(string $directory, int $permissions = 511, bool $recursive = false, mixed $context = null): bool
```

Makes a directory.

**Parameters**:
- `$directory` (`string`)
- `$permissions` (`int`), default `511`, optional
- `$recursive` (`bool`), default `false`, optional
- `$context` (`mixed`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/mkdir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/mkdir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mkdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/mkdir.md).
