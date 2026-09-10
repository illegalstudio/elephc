---
title: "mb_encoding_aliases()"
description: "Lists the public aliases of an encoding in PHP order."
sidebar:
  order: 809
---

## mb_encoding_aliases()

```php
function mb_encoding_aliases(string $encoding): array
```

Lists the public aliases of an encoding in PHP order.

**Parameters**:
- `$encoding` (`string`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_encoding_aliases.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_encoding_aliases.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_encoding_aliases` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_encoding_aliases.md).
