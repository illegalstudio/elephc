---
title: "finfo_file()"
description: "Identifies the type and encoding of a file or stream URL."
sidebar:
  order: 325
---

## finfo_file()

```php
function finfo_file(finfo $finfo, string $filename, int $flags = 0, ?resource $context = null): string|false
```

Identifies the type and encoding of a file or stream URL.

**Parameters**:
- `$finfo` (`finfo`)
- `$filename` (`string`)
- `$flags` (`int`), default `0`, optional
- `$context` (`?resource`), default `null`, optional

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_file.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `finfo_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/finfo_file.md).
