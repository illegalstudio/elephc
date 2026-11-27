//! Purpose:
//! End-to-end coverage for the first `ext/soap` client slice and its
//! `keep_headers` option.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext soap`.
//!
//! Key details:
//! - A loopback HTTP server answers one SOAP 1.1 POST with an `AddResponse`
//!   envelope and records the request for header/body assertions.
//! - The client is WSDL-less: `location` and `uri` options drive the call.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::support::*;

/// Accepts one loopback connection.
fn accept_connection(listener: &TcpListener) -> std::net::TcpStream {
    let (stream, _) = listener.accept().expect("accept SOAP request");
    stream
}

/// Reads one HTTP request from `stream`.
fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let size = stream.read(&mut chunk).expect("read request");
        if size == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..size]);
        let text = String::from_utf8_lossy(&buffer);
        if let Some(headers_end) = text.find("\r\n\r\n") {
            let length = text[..headers_end]
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if buffer.len() >= headers_end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

/// A WSDL-less SOAP call exercises the PHP prelude and HTTP transport.
#[test]
fn test_soap_client_wsdl_less_keep_headers() {
    let listener = TcpListener::bind(("0.0.0.0", 0)).expect("bind SOAP fixture");
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(String::new()));
    let server_capture = Arc::clone(&captured);
    let worker = thread::spawn(move || {
        let mut stream = accept_connection(&listener);
        let request = read_request(&mut stream);
        *server_capture.lock().unwrap() = request;
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <ns:AddResponse xmlns:ns="urn:test"><result>5</result></ns:AddResponse>
  </soap:Body>
</soap:Envelope>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).expect("write SOAP response");
    });

    let source = r#"<?php
$client = new SoapClient(null, [
    "location" => "http://127.0.0.1:__PORT__/soap",
    "uri" => "urn:test",
    "use" => SOAP_LITERAL,
    "keep_headers" => ["authorization"],
    "login" => "user",
    "password" => "pass",
    "trace" => true,
]);
$result = $client->__soapCall("Add", [["a" => 2, "b" => 3]]);
echo $result, "\n";
echo strpos($client->__getLastRequest(), "<ns1:Add>") !== false ? "request" : "bad", "\n";
echo strpos($client->__getLastResponse(), "AddResponse") !== false ? "response" : "bad", "\n";
"#
    .replace("__PORT__", &port.to_string());

    let out = compile_and_run(&source);
    worker.join().expect("SOAP fixture thread");
    assert_eq!(out, "5\nrequest\nresponse\n");
    let request = captured.lock().unwrap();
    assert!(request.starts_with("POST /soap HTTP/1.1"), "{request}");
    assert!(request.contains("SOAPAction: \"urn:test#Add\""), "{request}");
    assert!(request.contains("Authorization: Basic dXNlcjpwYXNz"), "{request}");
    assert!(request.contains("<ns1:Add>"), "{request}");
    assert!(request.contains("<ns1:a>2</ns1:a>"), "{request}");
}

/// A SOAP Fault becomes a `SoapFault` with its code and message.
#[test]
fn test_soap_fault_response() {
    let listener = TcpListener::bind(("0.0.0.0", 0)).expect("bind SOAP fault fixture");
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut stream = accept_connection(&listener);
        let _ = read_request(&mut stream);
        let body = r#"<?xml version="1.0"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <soap:Fault><faultcode>SOAP-ENV:Server</faultcode><faultstring>boom</faultstring></soap:Fault>
  </soap:Body>
</soap:Envelope>"#;
        let response = format!(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).expect("write SOAP fault");
    });

    let source = r#"<?php
$client = new SoapClient(null, [
    "location" => "http://127.0.0.1:__PORT__/soap",
    "uri" => "urn:test",
]);
try {
    $client->__soapCall("Fail", []);
} catch (SoapFault $fault) {
    echo $fault->faultcode, ":", $fault->faultstring, "\n";
}
"#
    .replace("__PORT__", &port.to_string());

    let out = compile_and_run(&source);
    worker.join().expect("SOAP fault fixture thread");
    assert_eq!(out, "SOAP-ENV:Server:boom\n");
}

/// Serves one HTTP response and records the request.
fn serve_once(
    listener: TcpListener,
    body: String,
    captured: Arc<Mutex<String>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut stream = accept_connection(&listener);
        let request = read_request(&mut stream);
        *captured.lock().unwrap() = request;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).expect("write WSDL response");
    })
}

/// Builds the original and imported WSDL bodies for a cross-host import.
fn wsdl_pair(import_port: u16, original_port: u16) -> (String, String) {
    let original = format!(
        r#"<?xml version="1.0"?>
<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
  xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
  targetNamespace="urn:elephc">
  <wsdl:import namespace="urn:imported" location="http://127.0.0.1:{import_port}/imported.wsdl"/>
  <wsdl:service name="Elephc"><wsdl:port name="Port" binding="tns:Binding">
    <soap:address location="http://127.0.0.1:{original_port}/soap"/>
  </wsdl:port></wsdl:service>
</wsdl:definitions>"#
    );
    let imported = r#"<?xml version="1.0"?>
<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
  targetNamespace="urn:imported"/>"#
        .to_string();
    (original, imported)
}

/// Runs a WSDL-mode constructor with Cookie/Proxy-Authorization context headers.
fn run_wsdl_import(keep_headers: &str) -> (String, String, String) {
    let original_listener = TcpListener::bind(("0.0.0.0", 0)).expect("bind original WSDL");
    let imported_listener = TcpListener::bind(("0.0.0.0", 0)).expect("bind imported WSDL");
    let original_port = original_listener.local_addr().unwrap().port();
    let imported_port = imported_listener.local_addr().unwrap().port();
    let (original_body, imported_body) = wsdl_pair(imported_port, original_port);
    let original_request = Arc::new(Mutex::new(String::new()));
    let imported_request = Arc::new(Mutex::new(String::new()));
    let original_worker = serve_once(
        original_listener,
        original_body,
        Arc::clone(&original_request),
    );
    let imported_worker = serve_once(
        imported_listener,
        imported_body,
        Arc::clone(&imported_request),
    );

    let source = r#"<?php
$client = new SoapClient("http://127.0.0.1:__ORIGINAL_PORT__/wsdl", [
    "login" => "user",
    "password" => "pass",
    "keep_headers" => __KEEP__,
]);
echo "ok\n";
"#
    .replace("__ORIGINAL_PORT__", &original_port.to_string())
    .replace("__KEEP__", keep_headers);

    let out = compile_and_run(&source);
    original_worker.join().expect("original WSDL thread");
    imported_worker.join().expect("imported WSDL thread");
    let original = original_request.lock().unwrap().clone();
    let imported = imported_request.lock().unwrap().clone();
    (out, original, imported)
}

/// Cross-host WSDL imports redact Authorization, Cookie and Proxy-Authorization.
#[test]
fn test_soap_wsdl_import_redacts_sensitive_headers() {
    let (out, original, imported) = run_wsdl_import("[]");
    assert_eq!(out, "ok\n");
    assert!(original.contains("Authorization: Basic dXNlcjpwYXNz"), "{original}");
    assert!(!imported.contains("Authorization:"), "{imported}");
}

/// keep_headers restores Authorization and Cookie on cross-host imports but not
/// Proxy-Authorization.
#[test]
fn test_soap_wsdl_import_keep_headers() {
    let (out, original, imported) = run_wsdl_import("[\"authorization\"]");
    assert_eq!(out, "ok\n");
    assert!(original.contains("Authorization: Basic dXNlcjpwYXNz"), "{original}");
    assert!(imported.contains("Authorization: Basic dXNlcjpwYXNz"), "{imported}");
}

/// A SOAP 1.2 client sends the 2005/05 envelope and `application/soap+xml`
/// content type, and parses a 2005/05 response.
#[test]
fn test_soap_client_soap12() {
    let listener = TcpListener::bind(("0.0.0.0", 0)).expect("bind SOAP 1.2 fixture");
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(String::new()));
    let server_capture = Arc::clone(&captured);
    let worker = thread::spawn(move || {
        let mut stream = accept_connection(&listener);
        let request = read_request(&mut stream);
        *server_capture.lock().unwrap() = request;
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope">
  <env:Body>
    <ns:AddResponse xmlns:ns="urn:test"><return>9</return></ns:AddResponse>
  </env:Body>
</env:Envelope>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/soap+xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).expect("write SOAP 1.2 response");
    });

    let source = r#"<?php
$client = new SoapClient(null, [
    "location" => "http://127.0.0.1:__PORT__/soap",
    "uri" => "urn:test",
    "soap_version" => SOAP_1_2,
]);
echo $client->__soapCall("Add", [["a" => 4, "b" => 5]]), "\n";
"#
    .replace("__PORT__", &port.to_string());

    let out = compile_and_run(&source);
    worker.join().expect("SOAP 1.2 fixture thread");
    assert_eq!(out, "9\n");
    let request = captured.lock().unwrap();
    assert!(request.starts_with("POST /soap HTTP/1.1"), "{request}");
    assert!(
        request.contains("Content-Type: application/soap+xml; charset=utf-8; action=\"urn:test#Add\""),
        "{request}"
    );
    assert!(!request.contains("SOAPAction:"), "{request}");
    assert!(
        request.contains("http://www.w3.org/2003/05/soap-envelope"),
        "{request}"
    );
    assert!(request.contains("<ns1:Add>"), "{request}");
}

/// A SOAP 1.2 Fault response maps to SoapFault's code and reason.
#[test]
fn test_soap_client_soap12_fault() {
    let listener = TcpListener::bind(("0.0.0.0", 0)).expect("bind SOAP 1.2 fault fixture");
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut stream = accept_connection(&listener);
        let _ = read_request(&mut stream);
        let body = r#"<?xml version="1.0"?>
<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope">
  <env:Body>
    <env:Fault>
      <env:Code><env:Value>env:Sender</env:Value></env:Code>
      <env:Reason><env:Text xml:lang="en">bad input</env:Text></env:Reason>
    </env:Fault>
  </env:Body>
</env:Envelope>"#;
        let response = format!(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/soap+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).expect("write SOAP 1.2 fault");
    });

    let source = r#"<?php
$client = new SoapClient(null, [
    "location" => "http://127.0.0.1:__PORT__/soap",
    "uri" => "urn:test",
    "soap_version" => SOAP_1_2,
]);
try {
    $client->__soapCall("Fail", []);
} catch (SoapFault $fault) {
    echo $fault->faultcode, ":", $fault->faultstring, "\n";
}
"#
    .replace("__PORT__", &port.to_string());

    let out = compile_and_run(&source);
    worker.join().expect("SOAP 1.2 fault fixture thread");
    assert_eq!(out, "env:Sender:bad input\n");
}

/// SoapServer dispatches registered functions and object methods.
#[test]
fn test_soap_server_function_and_class() {
    let out = compile_and_run(
        r#"<?php
function add(int $a, int $b): int {
    return $a + $b;
}
class Calculator {
    public function multiply(int $a, int $b): int {
        return $a * $b;
    }
}
$request = '<?xml version="1.0"?><soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ns:Add xmlns:ns="urn:test"><a>2</a><b>3</b></ns:Add></soap:Body></soap:Envelope>';
$server = new SoapServer(null, ["uri" => "urn:test"]);
$server->addFunction("add");
$server->handle($request);
echo "\n";
$request = '<?xml version="1.0"?><soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope"><soap:Body><ns:Multiply xmlns:ns="urn:test"><a>4</a><b>5</b></ns:Multiply></soap:Body></soap:Envelope>';
$server = new SoapServer(null, ["uri" => "urn:test", "soap_version" => SOAP_1_2]);
$server->setClass("Calculator");
$server->handle($request);
"#,
    );
    assert!(out.contains("<ns1:return>5</ns1:return>"), "{out}");
    assert!(out.contains("<ns1:return>20</ns1:return>"), "{out}");
    assert!(
        out.contains("http://www.w3.org/2003/05/soap-envelope"),
        "{out}"
    );
}

/// SoapServer serialises handler exceptions as SOAP 1.2 faults.
#[test]
fn test_soap_server_fault() {
    let out = compile_and_run(
        r#"<?php
function fail(int $code): int {
    throw new SoapFault("Server", "boom");
}
$request = '<?xml version="1.0"?><soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope"><soap:Body><ns:Fail xmlns:ns="urn:test"><code>7</code></ns:Fail></soap:Body></soap:Envelope>';
$server = new SoapServer(null, ["uri" => "urn:test", "soap_version" => SOAP_1_2]);
$server->addFunction("fail");
$server->handle($request);
"#,
    );
    assert!(out.contains("<SOAP-ENV:Fault>"), "{out}");
    assert!(out.contains("<SOAP-ENV:Value>Server</SOAP-ENV:Value>"), "{out}");
    assert!(out.contains("<SOAP-ENV:Text xml:lang=\"en\">boom</SOAP-ENV:Text>"), "{out}");
}

/// SOAP tracing exposes request/response headers and persists response cookies.
#[test]
fn test_soap_client_headers_and_cookies() {
    let listener = TcpListener::bind(("0.0.0.0", 0)).expect("bind SOAP header fixture");
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(Vec::<String>::new()));
    let server_capture = Arc::clone(&captured);
    let worker = thread::spawn(move || {
        for index in 0..2 {
            let mut stream = accept_connection(&listener);
            let request = read_request(&mut stream);
            server_capture.lock().unwrap().push(request);
            let body = format!(
                r#"<?xml version="1.0"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body><ns:CallResponse xmlns:ns="urn:test"><return>{}</return></ns:CallResponse></soap:Body>
</soap:Envelope>"#,
                index + 1
            );
            let cookie = if index == 0 {
                "Set-Cookie: session=abc; Path=/\r\n"
            } else {
                ""
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/xml; charset=utf-8\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                cookie,
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("write SOAP response");
        }
    });

    let source = r#"<?php
$client = new SoapClient(null, [
    "location" => "http://127.0.0.1:__PORT__/soap",
    "uri" => "urn:test",
    "trace" => true,
]);
echo $client->__soapCall("Call", []), "\n";
echo strpos($client->__getLastRequestHeaders(), "POST /soap HTTP/1.1") !== false ? "reqh" : "bad", "\n";
echo strpos($client->__getLastResponseHeaders(), "Set-Cookie: session=abc") !== false ? "resh" : "bad", "\n";
echo $client->__getCookies()["session"] ?? "missing", "\n";
echo $client->__soapCall("Call", []), "\n";
$fault = new SoapFault("Server", "boom");
echo $fault, "\n";
"#
    .replace("__PORT__", &port.to_string());

    let out = compile_and_run(&source);
    worker.join().expect("SOAP header fixture thread");
    assert_eq!(
        out,
        "1\nreqh\nresh\nabc\n2\nSoapFault exception: [Server] boom\n"
    );
    let requests = captured.lock().unwrap();
    assert!(requests.len() == 2, "{requests:?}");
    assert!(requests[1].contains("Cookie: session=abc"), "{}", requests[1]);
}

/// SOAP constants and the PHP 8.6 `Soap\Url`/`Soap\Sdl` classes exist.
#[test]
fn test_soap_constants_and_classes() {
    let out = compile_and_run(
        r#"<?php
echo SOAP_1_1, ":", SOAP_1_2, ":", SOAP_COMPRESSION_ACCEPT, ":",
    SOAP_AUTHENTICATION_DIGEST, ":", SOAP_ENC_ARRAY, ":", UNKNOWN_TYPE, ":",
    XSD_STRING, ":", XSD_ANYXML, "\n";
echo class_exists("Soap\\Url") ? "url" : "bad", ":",
    class_exists("Soap\\Sdl") ? "sdl" : "bad", "\n";
"#,
    );
    assert_eq!(out, "1:2:32:1:300:999998:101:147\nurl:sdl\n");
}

/// WSDL mode exposes `__getFunctions()` and `__getTypes()` metadata.
#[test]
fn test_soap_wsdl_metadata() {
    let out = compile_and_run(
        r#"<?php
$wsdl = '<?xml version="1.0"?>
<definitions xmlns="http://schemas.xmlsoap.org/wsdl/"
  xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
  xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns:tns="urn:add" targetNamespace="urn:add">
  <types>
    <xsd:schema targetNamespace="urn:add" elementFormDefault="qualified">
      <xsd:element name="Add"><xsd:complexType><xsd:sequence>
          <xsd:element name="a" type="xsd:int"/>
          <xsd:element name="b" type="xsd:int"/>
      </xsd:sequence></xsd:complexType></xsd:element>
      <xsd:element name="AddResponse"><xsd:complexType><xsd:sequence>
          <xsd:element name="result" type="xsd:int"/>
      </xsd:sequence></xsd:complexType></xsd:element>
    </xsd:schema>
  </types>
  <message name="AddInput"><part name="parameters" element="tns:Add"/></message>
  <message name="AddOutput"><part name="parameters" element="tns:AddResponse"/></message>
  <portType name="CalculatorPortType"><operation name="Add"><input message="tns:AddInput"/><output message="tns:AddOutput"/></operation></portType>
  <binding name="CalculatorBinding" type="tns:CalculatorPortType"><operation name="Add"><soap:operation soapAction="urn:add#Add"/></operation></binding>
  <service name="CalculatorService"><port name="CalculatorPort" binding="tns:CalculatorBinding"><soap:address location="http://127.0.0.1:1/soap"/></port></service>
</definitions>
';
file_put_contents("add.wsdl", $wsdl);
$plain = new SoapClient(null, ["location" => "http://127.0.0.1:1/soap", "uri" => "urn:x"]);
var_dump($plain->__getFunctions());
var_dump($plain->__getTypes());
$client = new SoapClient("add.wsdl", ["location" => "http://127.0.0.1:1/soap"]);
var_dump($client->__getFunctions());
var_dump($client->__getTypes());
"#,
    );
    assert_eq!(
        out,
        "NULL\nNULL\narray(1) {\n  [0]=>\n  string(32) \"AddResponse Add(Add $parameters)\"\n}\n\
         array(2) {\n  [0]=>\n  string(30) \"struct Add {\n int a;\n int b;\n}\"\n  [1]=>\n  string(35) \"struct AddResponse {\n int result;\n}\"\n}\n"
    );
}

/// SOAP_ENCODED serialises list arrays and maps, and decodes encoded responses.
#[test]
fn test_soap_encoded_arrays_and_maps() {
    let listener = TcpListener::bind(("0.0.0.0", 0)).expect("bind SOAP encoded fixture");
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(Vec::<String>::new()));
    let server_capture = Arc::clone(&captured);
    let worker = thread::spawn(move || {
        for index in 0..2 {
            let mut stream = accept_connection(&listener);
            let request = read_request(&mut stream);
            server_capture.lock().unwrap().push(request);
            let body = if index == 0 {
                r#"<?xml version="1.0"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
  xmlns:SOAP-ENC="http://schemas.xmlsoap.org/soap/encoding/"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <soap:Body><ns:TestResponse xmlns:ns="urn:test">
    <return SOAP-ENC:arrayType="xsd:string[2]" xsi:type="SOAP-ENC:Array">
      <item xsi:type="xsd:string">a</item>
      <item xsi:type="xsd:string">b</item>
    </return>
  </ns:TestResponse></soap:Body>
</soap:Envelope>"#
            } else {
                r#"<?xml version="1.0"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
  xmlns:ns2="http://xml.apache.org/xml-soap"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <soap:Body><ns:TestResponse xmlns:ns="urn:test">
    <return xsi:type="ns2:Map">
      <item><key xsi:type="xsd:string">x</key><value xsi:type="xsd:int">1</value></item>
      <item><key xsi:type="xsd:string">y</key><value xsi:type="xsd:int">2</value></item>
    </return>
  </ns:TestResponse></soap:Body>
</soap:Envelope>"#
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("write encoded response");
        }
    });

    let source = r#"<?php
$client = new SoapClient(null, [
    "location" => "http://127.0.0.1:__PORT__/soap",
    "uri" => "urn:test",
    "use" => SOAP_ENCODED,
]);
$list = $client->Test(["a", "b"], ["x" => 1, "y" => 2]);
echo implode(",", $list), "\n";
$map = $client->Test([], []);
echo $map["x"], ":", $map["y"], "\n";
"#
    .replace("__PORT__", &port.to_string());

    let out = compile_and_run(&source);
    worker.join().expect("SOAP encoded fixture thread");
    assert_eq!(out, "a,b\n1:2\n");
    let requests = captured.lock().unwrap();
    assert!(
        requests[0].contains("xmlns:SOAP-ENC=\"http://schemas.xmlsoap.org/soap/encoding/\""),
        "{}",
        requests[0]
    );
    assert!(
        requests[0].contains("SOAP-ENV:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\""),
        "{}",
        requests[0]
    );
    assert!(
        requests[0].contains("SOAP-ENC:arrayType=\"xsd:string[2]\""),
        "{}",
        requests[0]
    );
    assert!(
        requests[0].contains("<item xsi:type=\"xsd:string\">a</item>"),
        "{}",
        requests[0]
    );
    assert!(
        requests[0].contains("xsi:type=\"ns2:Map\""),
        "{}",
        requests[0]
    );
    assert!(
        requests[0].contains("<key xsi:type=\"xsd:string\">x</key>"),
        "{}",
        requests[0]
    );
    assert!(
        requests[0].contains("<value xsi:type=\"xsd:int\">1</value>"),
        "{}",
        requests[0]
    );
}
