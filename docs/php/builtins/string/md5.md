---
title: "md5()"
description: "Calculates the MD5 hash of a string."
sidebar:
  order: 391
---

## md5()

```php
function md5(string $string, bool $binary = false): string
```

Calculates the MD5 hash of a string.

**Parameters**:
- `$string` (`string`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/md5.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/md5.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `md5` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/md5.md).
