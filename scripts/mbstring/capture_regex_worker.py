#!/usr/bin/env python3
"""Capture PHP mbregex defaults across consecutive requests in the same native worker."""

import json
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
SOURCE = r'''<?php
if (PHP_VERSION !== '8.5.10' || MB_ONIGURUMA_VERSION !== '6.9.10') {
    throw new RuntimeException('Unexpected mbregex oracle version');
}
$before = [mb_regex_set_options(), mb_regex_encoding(), mb_ereg_match(".", "\xfa\x40")];
if (isset($_GET['first'])) {
    mb_regex_set_options('ib');
    mb_regex_encoding('ASCII');
    mb_ereg_search_init('aa', 'a');
    mb_ereg_search();
}
$state = [mb_regex_set_options(), mb_regex_encoding(), mb_ereg_search_getpos(), mb_ereg_search_getregs()];
try { $state[] = mb_ereg_search(); }
catch (Throwable $error) { $state[] = $error->getMessage(); }
echo json_encode(['before' => $before, 'state' => $state]);
'''


def capture(directive, value):
    """Retain one PHP process for two actual requests and always stop its listening server."""
    with tempfile.TemporaryDirectory() as directory:
        Path(directory, "index.php").write_text(SOURCE)
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        server = subprocess.Popen(
            ["php", "-d", "error_reporting=0", "-d", f"{directive}={value}",
             "-S", f"127.0.0.1:{port}", "-t", directory],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        try:
            for _ in range(100):
                try:
                    first = urllib.request.urlopen(f"http://127.0.0.1:{port}/?first=1", timeout=1).read()
                    break
                except OSError:
                    if server.poll() is not None:
                        raise
                    time.sleep(0.02)
            else:
                raise TimeoutError("PHP worker did not start")
            second = urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=2).read()
            return {"directive": directive, "input": value, "first": json.loads(first), "second": json.loads(second)}
        finally:
            server.terminate()
            server.wait(timeout=5)


def main():
    """Record configured defaults, ordinary worker state, and reset-time SJIS alias validation."""
    configurations = [
        ("mbstring.internal_encoding", "UTF-8"),
        ("mbstring.internal_encoding", "SJIS-WIN"),
        ("default_charset", "SJIS-WIN"),
        ("mbstring.internal_encoding", "Windows-1252"),
    ]
    results = [capture(*configuration) for configuration in configurations]
    destination = ROOT / "crates/elephc-mbstring/tests/fixtures/regex_worker.json"
    destination.write_text(json.dumps(results, indent=2) + "\n")
    print(f"Captured {len(results)} two-request worker configurations in {destination}")


if __name__ == "__main__":
    main()
