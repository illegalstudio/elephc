---
title: "fgetcsv()"
description: "Gets line from file pointer and parse for CSV fields."
sidebar:
  order: 163
---

## fgetcsv()

```php
function fgetcsv(resource $stream, int $length = null, string $separator = ','): array
```

Gets line from file pointer and parse for CSV fields.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`int`), default `null`, optional
- `$separator` (`string`), default `','`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fgetcsv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fgetcsv.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fgetcsv` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fgetcsv.md).

