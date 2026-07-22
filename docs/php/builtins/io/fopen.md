---
title: "fopen()"
description: "Opens file or URL."
sidebar:
  order: 171
---

## fopen()

```php
function fopen(string $filename, string $mode, bool $use_include_path = false, mixed $context = null): mixed
```

Opens file or URL.

**Parameters**:
- `$filename` (`string`)
- `$mode` (`string`)
- `$use_include_path` (`bool`), default `false`, optional
- `$context` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fopen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fopen.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fopen` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fopen.md).
