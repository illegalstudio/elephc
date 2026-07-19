---
title: "fwrite()"
description: "Binary-safe file write."
sidebar:
  order: 180
---

## fwrite()

```php
function fwrite(resource $stream, string $data): mixed
```

Binary-safe file write.

**Parameters**:
- `$stream` (`resource`)
- `$data` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fwrite.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fwrite.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fwrite` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fwrite.md).

