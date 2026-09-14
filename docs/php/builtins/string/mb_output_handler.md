---
title: "mb_output_handler()"
description: "Converts an output-buffer phase using the request encodings, MIME selection, and substitution settings."
sidebar:
  order: 831
---

## mb_output_handler()

```php
function mb_output_handler(string $string, int $status): string
```

Converts an output-buffer phase using the request encodings, MIME selection, and substitution settings.

**Parameters**:
- `$string` (`string`)
- `$status` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_output_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_output_handler.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_output_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_output_handler.md).
