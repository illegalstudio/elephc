---
title: "getservbyname()"
description: "Gets port number associated with an Internet service and protocol."
sidebar:
  order: 188
---

## getservbyname()

```php
function getservbyname(string $service, string $protocol): mixed
```

Gets port number associated with an Internet service and protocol.

**Parameters**:
- `$service` (`string`)
- `$protocol` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/getservbyname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/getservbyname.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getservbyname` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getservbyname.md).
