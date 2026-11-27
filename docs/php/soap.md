---
title: "SOAP"
description: "PHP's ext/soap client surface in elephc: WSDL and WSDL-less SoapClient calls, WSDL imports, SoapFault, and the PHP 8.6 keep_headers option."
sidebar:
  order: 26
---

elephc implements the `ext/soap` client as a PHP prelude. It supports both
WSDL-less calls (using the `location` and `uri` options) and WSDL mode, where
the client fetches the document, follows `wsdl:import`/`wsdl:include` and
`xsd:import`/`xsd:include` locations, and uses the first `soap:address` it finds
as the service endpoint. Everything is posted as a SOAP 1.1 envelope over a
TCP/TLS socket, so no libxml2 or native SOAP library is required.

## Delivered surface

| Area | API |
|---|---|
| Classes | `SoapClient`, `SoapServer`, `SoapFault`, `SoapHeader`, `SoapVar`, `SoapParam` |
| Client calls | `__soapCall()`, `__call()` |
| Server | `SoapServer::handle()`, `setClass()`, `setObject()`, `addFunction()`, `fault()`, `addSoapHeader()`, `setPersistence()`, `getFunctions()` |
| Introspection | `__getFunctions()`, `__getTypes()`, `__getLastRequest()`, `__getLastResponse()`, `__getLastRequestHeaders()`, `__getLastResponseHeaders()`, `__setLocation()`, `__setSoapHeaders()` |
| Transport hooks | `__doRequest()`, response-cookie capture through `__getCookies()`/`__setCookie()`, `Soap\Url`, `Soap\Sdl`, and the SOAP/XSD constant set |
| Options | `location`, `uri`, `style`, `use`, `soap_version`, `login`, `password`, `connection_timeout`, `trace`, `exceptions`, `user_agent`, `stream_context`, `keep_headers` |
| WSDL | WSDL 1.1 documents, recursive imports/includes, endpoint and target-namespace discovery, and `__getFunctions()`/`__getTypes()` operation/struct metadata |
| Transport | SOAP 1.1 and SOAP 1.2 envelopes, `http://` and `https://`/`tls://`, HTTP Basic authentication, request/response tracing |
| Constants | `SOAP_1_1`, `SOAP_1_2`, `SOAP_RPC`, `SOAP_DOCUMENT`, `SOAP_ENCODED`, `SOAP_LITERAL`, actor constants |

In WSDL mode the `location` and `uri` options override the discovered endpoint
and target namespace. WSDL fetches send the `stream_context` HTTP headers,
`User-Agent`, and Basic `Authorization` credentials.

`keep_headers` is validated and stored as a lowercase header-name list. PHP 8.6
applies it when a WSDL import or include points at a different origin than the
original WSDL. On a cross-origin fetch, elephc omits `Authorization`,
`Proxy-Authorization`, and `Cookie` unless the lowercase name is listed in
`keep_headers`. Same-origin imports receive every header.

```php
$client = new SoapClient("https://example.test/service.wsdl", [
    "login" => "user",
    "password" => "secret",
    "trace" => true,
    "keep_headers" => ["authorization"],
]);
$result = $client->__soapCall("Add", [["a" => 2, "b" => 3]]);
```

## Deferred surface

- Encoded arrays/objects (`SOAP_ENCODED`), MTOM and attachments.
- WSDL-described `SoapServer` operation metadata; the server dispatches
  RPC-style requests by operation name and returns `<return>` values.
- WSDL-described `SoapServer` operation metadata; the server dispatches
  RPC-style requests by operation name and returns `<return>` values.

Calling an undeclared member fails at compile time rather than pretending to
work.
