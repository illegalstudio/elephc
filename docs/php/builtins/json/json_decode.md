---
title: "json_decode()"
description: "Decodes a JSON string."
sidebar:
  order: 234
---

## json_decode()

```php
function json_decode(string $json, bool $associative = null, int $depth = 512, int $flags = 0): mixed
```

Decodes a JSON string.

**Parameters**:
- `$json` (`string`)
- `$associative` (`bool`), default `null`, optional
- `$depth` (`int`), default `512`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/json/json_decode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/json/json_decode.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_decode.md).

