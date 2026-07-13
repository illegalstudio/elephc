---
title: "fscanf()"
description: "Parses input from a file according to a format."
sidebar:
  order: 174
---

## fscanf()

```php
function fscanf(resource $stream, string $format, ...$vars): array
```

Parses input from a file according to a format.

**Parameters**:
- `$stream` (`resource`)
- `$format` (`string`)
- `...$vars` — variadic: collects excess arguments into `$vars`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fscanf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fscanf.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fscanf` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fscanf.md).

