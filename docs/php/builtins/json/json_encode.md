---
title: "json_encode()"
description: "Returns the JSON representation of a value."
sidebar:
  order: 248
---

## json_encode()

```php
function json_encode(mixed $value, int $flags = 0, int $depth = 512): string
```

Returns the JSON representation of a value.

**Parameters**:
- `$value` (`mixed`)
- `$flags` (`int`), default `0`, optional
- `$depth` (`int`), default `512`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/json/json_encode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/json/json_encode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_encode.md).

