---
title: "gmp_export()"
description: "Exports a GMP number as a binary string."
sidebar:
  order: 601
---

## gmp_export()

```php
function gmp_export(object|string|int $num, int $word_size = 1, int $flags = 17): string
```

Exports a GMP number as a binary string.

**Parameters**:
- `$num` (`object|string|int`)
- `$word_size` (`int`), default `1`, optional
- `$flags` (`int`), default `17`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_export.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_export.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_export` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_export.md).
