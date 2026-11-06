#!/usr/bin/env python3
"""Serve an offline interactive dashboard for every saved benchmark suite."""

from __future__ import annotations

import argparse
import json
import webbrowser
from functools import partial
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

from common import ROOT, SUITES
from history import load_history


def build_page(data_dir: Path) -> bytes:
    histories = {suite: load_history(data_dir / f"{suite}.json", suite) for suite in SUITES}
    data = json.dumps(histories).replace("<", "\\u003c").replace("&", "\\u0026")
    return (ROOT / "dashboard.html").read_text().replace("__DATA__", data).encode()


class Handler(BaseHTTPRequestHandler):
    """Return the embedded page without exposing the working directory."""

    def __init__(self, *args, page: bytes, **kwargs):
        self.page = page
        super().__init__(*args, **kwargs)

    def do_GET(self) -> None:
        if self.path not in ("/", "/index.html"):
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(self.page)))
        self.end_headers()
        self.wfile.write(self.page)

    def log_message(self, *_args) -> None:
        pass


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument("--no-open", action="store_true")
    parser.add_argument("--data-dir", type=Path, default=ROOT / "data")
    args = parser.parse_args()
    server = HTTPServer(("127.0.0.1", args.port), partial(Handler, page=build_page(args.data_dir)))
    url = f"http://127.0.0.1:{server.server_port}/"
    print(url, flush=True)
    if not args.no_open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
