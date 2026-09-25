<?php

// Windows-only GD screen capture. `imagegrabwindow($hwnd, true)` uses the same
// bridge when an application already owns a native HWND.
//
// Build on Windows, or cross-compile with MinGW-w64:
//   cargo run -- --target windows-x86_64 examples/windows-image-capture/main.php
//   ./examples/windows-image-capture/main.exe

$desktop = imagegrabscreen();
if ($desktop === false) {
    echo "Desktop capture failed\n";
    exit(1);
}

$path = __DIR__ . "\\desktop.png";
imagepng($desktop, $path);
echo "Captured " . imagesx($desktop) . "x" . imagesy($desktop) . " to " . $path . "\n";
imagedestroy($desktop);
