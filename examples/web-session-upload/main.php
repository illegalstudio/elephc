<?php
// elephc-web: session.upload_progress — real streaming upload progress.
//
// Compile: cargo run -- --web examples/web-session-upload/main.php
// Run:     ./examples/web-session-upload/main --listen 127.0.0.1:8080
// Try:     open http://127.0.0.1:8080/ in a browser, pick a file of a few MB,
//          and watch the progress bar fill from the concurrent /progress poll.
//          Over a real (or throttled) connection the bar climbs; on a fast
//          loopback the transfer may finish before the first poll lands.
//
// Note: each request buffers its whole body in the per-request heap, so keep
// demo uploads to a few MB (larger files exhaust the heap). The server also
// rejects bodies over --max-body-size (default 8 MiB) with 413 before the
// handler runs.
//
// How it works
// ------------
// A session tracks the upload. The browser POSTs a multipart form to /upload;
// while the server streams that request body in, it writes a progress array
// into the session file under $_SESSION['upload_progress_' . <key>]. A *second*,
// concurrent request (GET /progress) reads that entry to report how far the
// upload has advanced. The upload request itself only sees the finished result —
// its own body is fully buffered before its handler runs, and with
// session.upload_progress.cleanup on (the default) the entry is removed once the
// body is complete. That mirrors PHP exactly.
//
// The hidden PHP_SESSION_UPLOAD_PROGRESS field MUST appear before the file field
// in the form, so the server learns the progress key before the bytes arrive.

// Every route joins the same session so the cookie set on GET / is carried by
// both the upload POST and the concurrent progress poll.
session_start();

$method = $_SERVER['REQUEST_METHOD'];
$path = $_SERVER['REQUEST_URI'];
$q = strpos($path, '?');
if ($q !== false) {
    $path = substr($path, 0, $q);
}

// The upload-progress key the form advertises. $_SESSION stores the live entry
// under session.upload_progress.prefix . <this value> (default prefix
// "upload_progress_").
$progress_key = 'demo';

if ($method === 'GET' && $path === '/') {
    // The upload page: a form plus a tiny poller. XHR streams the file to
    // /upload; every 200ms we fetch /progress to update the bar.
    header('Content-Type: text/html; charset=utf-8');
    echo '<!doctype html><html><head><meta charset="utf-8"><title>elephc upload progress</title></head><body>';
    echo '<h1>Streaming upload progress</h1>';
    echo '<form id="f" method="post" action="/upload" enctype="multipart/form-data">';
    // MUST come before the file input.
    echo '<input type="hidden" name="PHP_SESSION_UPLOAD_PROGRESS" value="' . $progress_key . '">';
    echo '<input type="file" name="file"> <button type="submit">Upload</button>';
    echo '</form>';
    echo '<progress id="bar" value="0" max="100" style="width:20em"></progress> <span id="pct">0%</span>';
    echo '<pre id="out"></pre>';
    echo '<script>
const f = document.getElementById("f"), bar = document.getElementById("bar"),
      pct = document.getElementById("pct"), out = document.getElementById("out");
f.addEventListener("submit", e => {
  e.preventDefault();
  const xhr = new XMLHttpRequest();
  xhr.open("POST", "/upload");
  xhr.onload = () => { out.textContent = xhr.responseText; poll.stop = true; };
  xhr.send(new FormData(f));
  const poll = { stop: false };
  const tick = async () => {
    if (poll.stop) return;
    const r = await fetch("/progress");
    const p = await r.json();
    if (p && p.content_length > 0) {
      const v = Math.round(100 * p.bytes_processed / p.content_length);
      bar.value = v; pct.textContent = v + "%";
    }
    setTimeout(tick, 200);
  };
  tick();
});
</script>';
    echo '</body></html>';
} elseif ($method === 'POST' && $path === '/upload') {
    // The upload handler. By the time this runs the body is fully received, so
    // $_FILES is populated and the progress entry has already been cleaned up.
    header('Content-Type: text/plain; charset=utf-8');
    if (isset($_FILES['file'])) {
        $file = $_FILES['file'];
        echo "Received: " . $file['name'] . " (" . $file['size'] . " bytes, type " . $file['type'] . ")\n";
    } else {
        echo "No file field received.\n";
    }
} elseif ($method === 'GET' && $path === '/progress') {
    // The concurrent poller. While the upload streams in, this reads the live
    // progress entry the server is writing to the session file.
    header('Content-Type: application/json');
    $entry = $_SESSION['upload_progress_' . $progress_key] ?? null;
    if ($entry === null) {
        // No upload in flight (or it already finished and was cleaned up).
        echo '{"bytes_processed":0,"content_length":0,"done":true}';
    } else {
        echo json_encode($entry);
    }
} else {
    http_response_code(404);
    header('Content-Type: text/plain; charset=utf-8');
    echo "404 Not Found\n";
}
