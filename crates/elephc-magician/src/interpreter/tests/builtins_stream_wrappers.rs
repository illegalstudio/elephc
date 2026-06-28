//! Purpose:
//! Interpreter tests for eval-supported stream wrapper URL handling.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - PHAR fixtures are written through `elephc-phar` so tests exercise the same
//!   archive bridge used by generated-runtime paths.
//! - HTTP tests use a one-shot localhost server to avoid external network dependencies.

use super::super::*;
use super::support::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

/// Verifies eval `fopen()` and one-shot file builtins handle supported wrappers.
#[test]
fn execute_program_dispatches_supported_stream_wrapper_urls() {
    let pid = std::process::id();
    let local = format!("elephc_magician_wrapper_local_{pid}.txt");
    let archive = format!("elephc_magician_wrapper_{pid}.phar");
    let read_url = format!("phar://{archive}/dir/read.txt");
    let put_url = format!("phar://{archive}/dir/put.txt");
    let stream_url = format!("phar://{archive}/dir/stream.txt");
    let _ = std::fs::remove_file(&local);
    let _ = std::fs::remove_file(&archive);
    std::fs::write(&local, b"local").expect("write local wrapper fixture");
    elephc_phar::put_url_bytes(read_url.as_bytes(), b"from-phar")
        .expect("write phar wrapper fixture");
    let source = format!(
        r#"echo file_get_contents("file://{local}") === "local" ? "fileurl" : "bad"; echo ":";
$memory = fopen("php://memory", "w+");
fwrite($memory, "mem");
rewind($memory);
echo fread($memory, 3) === "mem" ? "memory" : "bad"; echo ":";
fclose($memory);
$data = fopen("data://text/plain;base64,SGVsbG8=", "r");
echo fread($data, 5) === "Hello" ? "data" : "bad"; echo ":";
fclose($data);
$phar = fopen("{read_url}", "r");
echo fread($phar, 32) === "from-phar" ? "pharopen" : "bad"; echo ":";
fclose($phar);
echo file_get_contents("{read_url}") === "from-phar" ? "pharget" : "bad"; echo ":";
echo file_exists("{read_url}") && is_file("{read_url}") && is_readable("{read_url}") ? "pharprobe" : "bad"; echo ":";
echo filetype("{read_url}") === "file" ? "phartype" : "bad"; echo ":";
echo filesize("{read_url}") === 9 ? "pharsize" : "bad"; echo ":";
echo file_put_contents("{put_url}", "put") === 3 ? "pharput" : "bad"; echo ":";
echo file_get_contents("{put_url}") === "put" ? "putread" : "bad"; echo ":";
$out = fopen("{stream_url}", "w");
fwrite($out, "stream");
echo fclose($out) ? "streamclose" : "bad"; echo ":";
echo file_get_contents("{stream_url}") === "stream" ? "streamread" : "bad"; echo ":";
echo unlink("{stream_url}") ? "unlink" : "bad"; echo ":";
echo file_get_contents("{stream_url}") === false ? "deleted" : "bad";
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&local);
    let _ = std::fs::remove_file(&archive);
    assert_eq!(
        values.output,
        "fileurl:memory:data:pharopen:pharget:pharprobe:phartype:pharsize:pharput:putread:streamclose:streamread:unlink:deleted"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `http://` URLs feed `file_get_contents()` and `fopen()`.
#[test]
fn execute_program_dispatches_http_stream_wrapper_urls() {
    let (fgc_port, fgc_server) = spawn_http_once("fgc-body");
    let (fopen_port, fopen_server) = spawn_http_once("stream-body");
    let source = format!(
        r#"echo file_get_contents("http://127.0.0.1:{fgc_port}/body?x=1") === "fgc-body" ? "fgc" : "bad"; echo ":";
$h = fopen("http://127.0.0.1:{fopen_port}/stream", "r");
echo is_resource($h) ? "open" : "bad"; echo ":";
echo fread($h, 64) === "stream-body" ? "read" : "bad"; echo ":";
echo fclose($h) ? "close" : "bad"; echo ":";
echo file_get_contents("http://") === false ? "invalid" : "bad";
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    fgc_server.join().expect("join file_get_contents HTTP fixture");
    fopen_server.join().expect("join fopen HTTP fixture");
    assert_eq!(values.output, "fgc:open:read:close:invalid");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval stream wrapper registration changes the visible wrapper list.
#[test]
fn execute_program_tracks_stream_wrapper_registry_state() {
    let program = parse_fragment(
        br#"$before = stream_get_wrappers();
echo in_array("evaltest", $before) ? "bad" : "missing"; echo ":";
echo stream_wrapper_register("evaltest", "stdClass") ? "reg" : "bad"; echo ":";
$after = stream_get_wrappers();
echo in_array("evaltest", $after) ? "listed" : "bad"; echo ":";
echo stream_wrapper_unregister("evaltest") ? "unreg" : "bad"; echo ":";
$removed = call_user_func("stream_get_wrappers");
echo in_array("evaltest", $removed) ? "bad" : "removed"; echo ":";
echo stream_wrapper_unregister("file") ? "unfile" : "bad"; echo ":";
$without_file = stream_get_wrappers();
echo in_array("file", $without_file) ? "bad" : "nofile"; echo ":";
echo stream_wrapper_restore("file") ? "restore" : "bad"; echo ":";
$restored = call_user_func_array("stream_get_wrappers", []);
echo in_array("file", $restored) ? "fileback" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "missing:reg:listed:unreg:removed:unfile:nofile:restore:fileback"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval userspace stream wrappers dispatch core stream methods.
#[test]
fn execute_program_dispatches_user_stream_wrapper_methods() {
    let program = parse_fragment(
        br#"class EvalUserWrapperW {
    public $data;
    public $pos;
    public function stream_open($path, $mode, $options, &$opened_path): bool {
        $this->data = "abcdef";
        $this->pos = 0;
        $opened_path = "ignored";
        return true;
    }
    public function stream_read($count): string {
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos += strlen($chunk);
        return $chunk;
    }
    public function stream_write($data): int {
        echo "[" . $data . "]";
        return strlen($data);
    }
    public function stream_eof(): bool {
        return $this->pos >= strlen($this->data);
    }
    public function stream_close(): void {
        echo "C";
    }
}
echo stream_wrapper_register("uw", "EvalUserWrapperW") ? "reg" : "bad"; echo ":";
$h = fopen("uw://read", "r");
echo is_resource($h) ? "open" : "bad"; echo ":";
echo fread($h, 2); echo ":";
echo feof($h) ? "bad" : "not"; echo ":";
echo fread($h, 4); echo ":";
echo feof($h) ? "eof" : "bad"; echo ":";
echo fclose($h) ? "closed" : "bad"; echo ":";
$w = fopen("uw://write", "w");
echo fwrite($w, "xyz") === 3 ? "wrote" : "bad"; echo ":";
echo fclose($w) ? "closed2" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "reg:open:ab:not:cdef:eof:Cclosed:[xyz]wrote:Cclosed2"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a wrapper whose `stream_open()` returns false makes `fopen()` false.
#[test]
fn execute_program_user_stream_wrapper_open_false_returns_false() {
    let program = parse_fragment(
        br#"class EvalRejectWrapperW {
    public function stream_open($path, $mode, $options, &$opened_path): bool {
        return false;
    }
}
stream_wrapper_register("rejectw", "EvalRejectWrapperW");
echo fopen("rejectw://x", "r") === false ? "false" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "false");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies aggregate stream helpers drain and copy userspace wrapper streams.
#[test]
fn execute_program_dispatches_user_stream_wrapper_aggregate_reads() {
    let program = parse_fragment(
        br#"class EvalSlowWrapperW {
    public $data;
    public $pos;
    public function stream_open($path, $mode, $options, &$opened_path): bool {
        $this->data = "abcdefghi";
        $this->pos = 0;
        return true;
    }
    public function stream_read($count): string {
        $limit = min(2, $count);
        $chunk = substr($this->data, $this->pos, $limit);
        $this->pos += strlen($chunk);
        return $chunk;
    }
    public function stream_eof(): bool {
        return $this->pos >= strlen($this->data);
    }
    public function stream_seek($offset, $whence): bool {
        if ($whence !== 0) { return false; }
        $this->pos = $offset;
        return true;
    }
}
class EvalSinkWrapperW {
    public function stream_open($path, $mode, $options, &$opened_path): bool {
        return true;
    }
    public function stream_write($data): int {
        echo "<" . $data . ">";
        return strlen($data);
    }
}
stream_wrapper_register("sloww", "EvalSlowWrapperW");
stream_wrapper_register("sinkw", "EvalSinkWrapperW");
$h = fopen("sloww://read", "r");
echo stream_get_contents($h, 5) === "abcde" ? "bounded" : "bad"; echo ":";
echo stream_get_contents($h) === "fghi" ? "rest" : "bad"; echo ":";
echo stream_get_contents($h, 3, 2) === "cde" ? "offset" : "bad"; echo ":";
$src = fopen("sloww://copy", "r");
$dst = fopen("php://memory", "w+");
echo stream_copy_to_stream($src, $dst, 5) === 5 ? "copy" : "bad"; echo ":";
rewind($dst);
echo stream_get_contents($dst) === "abcde" ? "copied" : "bad"; echo ":";
$raw = fopen("php://memory", "w+");
fwrite($raw, "sinkdata");
rewind($raw);
$sink = fopen("sinkw://out", "w");
echo stream_copy_to_stream($raw, $sink) === 8 ? "sinkcopy" : "bad"; echo ":";
$pass = fopen("sloww://pass", "r");
echo fpassthru($pass) === 9 ? "pass" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "bounded:rest:offset:copy:copied:<sinkdata>sinkcopy:abcdefghipass"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies path-based filesystem builtins dispatch to wrapper `url_stat()`.
#[test]
fn execute_program_dispatches_user_stream_wrapper_url_stat() {
    let program = parse_fragment(
        br#"class EvalUrlStatWrapperW {
    public function url_stat($path, $flags) {
        if (str_contains($path, "missing")) {
            return false;
        }
        if (str_contains($path, "dir")) {
            return ["mode" => 16877, "size" => 0, "mtime" => 5, "uid" => 10];
        }
        return [
            "mode" => 33188,
            "size" => 123,
            "mtime" => 77,
            "atime" => 66,
            "ctime" => 88,
            "uid" => 501,
            "gid" => 20,
            "ino" => 9
        ];
    }
}
stream_wrapper_register("ustat", "EvalUrlStatWrapperW");
echo file_exists("ustat://file") ? "exists" : "bad"; echo ":";
echo is_file("ustat://file") ? "file" : "bad"; echo ":";
echo is_dir("ustat://file") ? "bad" : "notdir"; echo ":";
echo filetype("ustat://file") === "file" ? "type" : "bad"; echo ":";
echo filesize("ustat://file") === 123 ? "size" : "bad"; echo ":";
echo filemtime("ustat://file") === 77 ? "mtime" : "bad"; echo ":";
echo fileowner("ustat://file") === 501 ? "owner" : "bad"; echo ":";
$stat = stat("ustat://file");
echo $stat["size"] === 123 && $stat["mode"] === 33188 ? "stat" : "bad"; echo ":";
echo call_user_func("filesize", "ustat://file") === 123 ? "callsize" : "bad"; echo ":";
echo file_exists("ustat://missing") ? "bad" : "missing"; echo ":";
echo filetype("ustat://dir") === "dir" ? "dirtype" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "exists:file:notdir:type:size:mtime:owner:stat:callsize:missing:dirtype"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Starts a localhost HTTP server that returns one fixed body and then exits.
fn spawn_http_once(body: &'static str) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind HTTP wrapper fixture");
    let port = listener
        .local_addr()
        .expect("read HTTP wrapper fixture address")
        .port();
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept HTTP wrapper request");
        let mut request = [0_u8; 1024];
        let _ = socket.read(&mut request).expect("read HTTP wrapper request");
        let response = format!(
            "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket
            .write_all(response.as_bytes())
            .expect("write HTTP wrapper response");
    });
    (port, handle)
}
