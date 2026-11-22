#!/bin/bash
# Build, run, or self-test the elephc http-server showcase.
#
# Usage (from anywhere):
#   ./showcases/http-server/build.sh          compile the server
#   ./showcases/http-server/build.sh run      compile, then run it
#   ./showcases/http-server/build.sh test     compile, run, hit every route, stop
#
# While the server is running, test it by hand from another terminal:
#   curl http://127.0.0.1:8080/
#   curl 'http://127.0.0.1:8080/hello?name=elephc'
#   curl http://127.0.0.1:8080/json
#   curl http://127.0.0.1:8080/stats

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
PORT=8080
BIN="$SCRIPT_DIR/main"

# -- build the compiler, then compile the showcase --
echo "==> Building the elephc compiler"
( cd "$ROOT_DIR" && cargo build --release )

echo "==> Compiling the HTTP server showcase"
"$ROOT_DIR/target/release/elephc" build "$SCRIPT_DIR/main.php"
echo "==> Built: $BIN"

case "$1" in
    run)
        echo "==> Starting the server (Ctrl+C to stop)"
        exec "$BIN"
        ;;

    test)
        echo "==> Starting the server for a self-test"
        "$BIN" &
        SRV=$!
        sleep 1
        echo
        for path in "/" "/hello?name=elephc" "/json" "/stats" "/missing"; do
            reply=$(curl -s --max-time 5 -w $'\n%{http_code}' "http://127.0.0.1:$PORT$path" || true)
            code=$(printf '%s' "$reply" | tail -n1)
            body=$(printf '%s' "$reply" | head -n1)
            printf '  %-24s [%s]  %s\n' "$path" "$code" "$body"
        done
        echo
        echo "==> Stopping the server"
        kill "$SRV" 2>/dev/null || true
        ;;

    *)
        echo
        echo "Run the server:"
        echo "  $BIN"
        echo
        echo "Then, from another terminal:"
        echo "  curl http://127.0.0.1:$PORT/"
        echo "  curl 'http://127.0.0.1:$PORT/hello?name=elephc'"
        echo "  curl http://127.0.0.1:$PORT/json"
        echo "  curl http://127.0.0.1:$PORT/stats"
        echo
        echo "Shortcuts:"
        echo "  $0 run     build + run"
        echo "  $0 test    build + run + check every route automatically"
        ;;
esac
