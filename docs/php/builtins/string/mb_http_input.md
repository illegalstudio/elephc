---
title: "mb_http_input()"
description: "Returns recorded HTTP input encoding identification or the configured input encoding list."
sidebar:
  order: 823
---

## mb_http_input()

```php
function mb_http_input(?string $type = null): array|string|false
```

Returns recorded HTTP input encoding identification or the configured input encoding list.

**Parameters**:
- `$type` (`?string`), default `null`, optional

**Returns**: `array|string|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_http_input.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_http_input.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_http_input` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_http_input.md).
