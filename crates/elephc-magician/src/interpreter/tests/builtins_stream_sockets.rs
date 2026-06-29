//! Purpose:
//! Interpreter tests for eval TCP stream socket builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Tests bind localhost port 0 so the OS chooses available ports.
//! - Socket resources are then exercised through regular eval stream reads/writes.

use super::super::*;
use super::support::*;

/// Verifies stream socket helpers support local TCP and socket pairs.
#[test]
fn execute_program_dispatches_stream_socket_builtins() {
    let program = parse_fragment(
        br#"$server = stream_socket_server("tcp://127.0.0.1:0");
echo is_resource($server) ? "server" : "bad"; echo ":";
$addr = stream_socket_get_name($server, false);
echo $addr !== false ? "name" : "bad"; echo ":";
$client = stream_socket_client("tcp://" . $addr);
echo is_resource($client) ? "client" : "bad"; echo ":";
$peerName = "";
$peer = stream_socket_accept($server, null, $peerName);
echo is_resource($peer) ? "accept" : "bad"; echo ":";
echo $peerName !== "" ? "peerout" : "bad"; echo ":";
echo stream_socket_get_name($client, true) !== false ? "peername" : "bad"; echo ":";
echo stream_socket_sendto($client, "ping") === 4 ? "send" : "bad"; echo ":";
$remoteAddr = "";
echo stream_socket_recvfrom($peer, 4, 0, $remoteAddr) === "ping" ? "recv" : "bad"; echo ":";
echo $remoteAddr !== "" ? "addrout" : "bad"; echo ":";
fwrite($peer, "pong");
echo fread($client, 4) === "pong" ? "roundtrip" : "bad"; echo ":";
echo stream_socket_enable_crypto($client, false) ? "cryptooff" : "bad"; echo ":";
echo stream_socket_shutdown($client, 2) ? "shutdown" : "bad"; echo ":";
fclose($peer); fclose($client); fclose($server);
$server = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($server, false);
$parts = explode(":", $addr);
$fs = fsockopen("127.0.0.1", intval($parts[1]));
$peer = stream_socket_accept($server);
echo is_resource($fs) && is_resource($peer) ? "fsock" : "bad"; echo ":";
fclose($fs); fclose($peer); fclose($server);
$server = stream_socket_server("tcp://127.0.0.1:0");
$addr = call_user_func("stream_socket_get_name", $server, false);
$parts = explode(":", $addr);
$pfs = pfsockopen("127.0.0.1", intval($parts[1]));
$peer = stream_socket_accept($server);
echo is_resource($pfs) && is_resource($peer) ? "pfsock" : "bad"; echo ":";
fclose($pfs); fclose($peer); fclose($server);
$server = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($server, false);
$accept = "stream_socket_accept";
$client = stream_socket_client("tcp://" . $addr);
$dynPeer = "";
$peer = $accept($server, null, $dynPeer);
echo is_resource($peer) && $dynPeer !== "" ? "dynaccept" : "bad"; echo ":";
fwrite($client, "call");
$recv = stream_socket_recvfrom(...);
$dynAddr = "";
echo $recv($peer, 4, 0, $dynAddr) === "call" && $dynAddr !== "" ? "dynrecv" : "bad"; echo ":";
fclose($client); fclose($peer); fclose($server);
$pair = stream_socket_pair(1, 1, 0);
echo is_array($pair) && is_resource($pair[0]) && is_resource($pair[1]) ? "pair" : "bad"; echo ":";
fwrite($pair[0], "xy");
echo fread($pair[1], 2) === "xy" ? "pairio" : "bad"; echo ":";
fclose($pair[0]); fclose($pair[1]);
$read = []; $write = []; $except = [];
echo stream_select($read, $write, $except, 0) === 0 ? "select" : "bad"; echo ":";
echo function_exists("fsockopen"); echo function_exists("pfsockopen");
echo function_exists("stream_select"); echo function_exists("stream_socket_accept");
echo function_exists("stream_socket_client"); echo function_exists("stream_socket_enable_crypto");
echo function_exists("stream_socket_get_name"); echo function_exists("stream_socket_pair");
echo function_exists("stream_socket_recvfrom"); echo function_exists("stream_socket_sendto");
echo function_exists("stream_socket_server"); echo function_exists("stream_socket_shutdown");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "server:name:client:accept:peerout:peername:send:recv:addrout:",
            "roundtrip:cryptooff:shutdown:fsock:pfsock:dynaccept:dynrecv:",
            "pair:pairio:select:111111111111"
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
