---
title: "json_validate()"
description: "Checks if a string contains valid JSON."
sidebar:
  order: 238
---

## json_validate()

```php
function json_validate(string $json, int $depth = 512, int $flags = 0): bool
```

Checks if a string contains valid JSON.

**Parameters**:
- `$json` (`string`)
- `$depth` (`int`), default `512`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/json/json_validate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/json/json_validate.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `json_validate` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_validate.md).

