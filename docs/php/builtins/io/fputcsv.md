---
title: "fputcsv()"
description: "Format line as CSV and write to file pointer."
sidebar:
  order: 172
---

## fputcsv()

```php
function fputcsv(resource $stream, array $fields, string $separator = ',', string $enclosure = '"'): int
```

Format line as CSV and write to file pointer.

**Parameters**:
- `$stream` (`resource`)
- `$fields` (`array`)
- `$separator` (`string`), default `','`, optional
- `$enclosure` (`string`), default `'"'`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fputcsv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fputcsv.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fputcsv` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fputcsv.md).

