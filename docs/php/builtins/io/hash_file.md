---
title: "hash_file()"
description: "Generates a hash value using the contents of a given file."
sidebar:
  order: 190
---

## hash_file()

```php
function hash_file(string $algo, string $filename, bool $binary = false): mixed
```

Generates a hash value using the contents of a given file.

**Parameters**:
- `$algo` (`string`)
- `$filename` (`string`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_file.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/hash_file.md).
