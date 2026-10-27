---
title: "finfo_set_flags()"
description: "Changes the persistent fileinfo configuration flags."
sidebar:
  order: 325
---

## finfo_set_flags()

```php
function finfo_set_flags(finfo $finfo, int $flags): bool
```

Changes the persistent fileinfo configuration flags.

**Parameters**:
- `$finfo` (`finfo`)
- `$flags` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_set_flags.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/fileinfo/finfo_set_flags.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `finfo_set_flags` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/finfo_set_flags.md).
