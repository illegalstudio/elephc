---
title: "header()"
description: "Sends a raw HTTP header."
sidebar:
  order: 295
---

## header()

```php
function header(string $header, bool $replace = true, int $response_code = 0): void
```

Sends a raw HTTP header.

**Parameters**:
- `$header` (`string`)
- `$replace` (`bool`), default `true`, optional
- `$response_code` (`int`), default `0`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/header.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/header.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `header` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/header.md).
