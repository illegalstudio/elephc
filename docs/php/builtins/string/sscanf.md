---
title: "sscanf()"
description: "Parses a string according to a format."
sidebar:
  order: 401
---

## sscanf()

```php
function sscanf(string $string, string $format, ...$vars): array
```

Parses a string according to a format.

**Parameters**:
- `$string` (`string`)
- `$format` (`string`)
- `...$vars` — variadic: collects excess arguments into `$vars`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/formatting/sscanf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/sscanf.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sscanf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sscanf.md).
