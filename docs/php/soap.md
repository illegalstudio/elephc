---
title: "SOAP"
description: "PHP's ext/soap client surface in elephc: WSDL-less SoapClient calls, SoapFault, and the PHP 8.6 keep_headers option."
sidebar:
  order: 26
---

elephc implements the first `ext/soap` client slice as a PHP prelude. It is
WSDL-less: calls use the `location` and `uri` constructor options and post a
SOAP 1.1 envelope over a TCP/TLS socket, so no libxml2 or native SOAP library is
required.

## Delivered surface

| Area | API |
|---|---|
| Classes | `SoapClient`, `SoapFault`, `SoapHeader`, `SoapVar`, `SoapParam` |
| Calls | `__soapCall()`, `__call()` |
| Introspection | `__getFunctions()`, `__getTypes()`, `__getLastRequest()`, `__getLastResponse()`, `__setLocation()`, `__setSoapHeaders()` |
| Options | `location`, `uri`, `style`, `use`, `login`, `password`, `connection_timeout`, `trace`, `exceptions`, `user_agent`, `keep_headers` |
| Transport | `http://` and `tls://`, HTTP Basic authentication, request/response tracing |
| Constants | `SOAP_1_1`, `SOAP_1_2`, `SOAP_RPC`, `SOAP_DOCUMENT`, `SOAP_ENCODED`, `SOAP_LITERAL`, actor constants |

`keep_headers` is validated and stored as a lowercase header-name list. PHP
8.6 applies it when importing a WSDL from a different host; WSDL imports are
not implemented yet, so in the WSDL-less client the option has no request
effect.

```php
$client = new SoapClient(null, [
    "location" => "http://127.0.0.1:8080/soap",
    "uri" => "urn:example",
    "trace" => true,
]);
$result = $client->__soapCall("Add", [["a" => 2, "b" => 3]]);
```

## Deferred surface

- WSDL parsing and imports, including the PHP 8.6 cross-host `keep_headers`
  redaction behavior.
- SOAP 1.2 envelopes, encoded arrays/objects (`SOAP_ENCODED`), MTOM and
  attachments.
- Persistent cookies, `__getCookies()` population, `__setCookie()` behavior,
  and `__getFunctions()`/`__getTypes()` metadata.
- `SoapServer`.

Calling an undeclared member fails at compile time rather than pretending to
work.
