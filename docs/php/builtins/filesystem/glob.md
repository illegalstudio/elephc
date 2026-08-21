---
title: "glob()"
description: "Finds pathnames matching a pattern."
sidebar:
  order: 134
---

## glob()

```php
function glob(string $pattern): mixed
```

Finds pathnames matching a pattern.

**Parameters**:
- `$pattern` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/glob.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/glob.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `glob` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/glob.md).
