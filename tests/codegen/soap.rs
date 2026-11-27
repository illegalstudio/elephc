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
use std::time::{Duration, Instant};

use crate::support::*;

/// Accepts one loopback connection, failing the fixture after five seconds.
fn accept_with_timeout(listener: &TcpListener) -> std::net::TcpStream {
    listener.set_nonblocking(true).expect("nonblocking listener");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).expect("blocking stream");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    panic!("SOAP fixture timed out waiting for a request");
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accept SOAP request: {error}"),
        }
    }
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
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind SOAP fixture");
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(String::new()));
    let server_capture = Arc::clone(&captured);
    let worker = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener);
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
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind SOAP fault fixture");
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut stream = accept_with_timeout(&listener);
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
