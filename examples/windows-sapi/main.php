<?php

// Windows-only SAPI helpers: inspect code pages, convert legacy bytes, query
// virtual-terminal support, and install a safe deferred Ctrl-C/Ctrl-Break handler.
//
// Build on Windows, or cross-compile with MinGW-w64:
//   cargo run -- --target windows-x86_64 examples/windows-sapi/main.php
//   ./examples/windows-sapi/main.exe

echo "current code page: " . sapi_windows_cp_get() . "\n";
echo "ANSI/OEM: " . sapi_windows_cp_get("ansi")
    . "/" . sapi_windows_cp_get("oem") . "\n";

$windows1252 = "caf\xe9";
$utf8 = sapi_windows_cp_conv("windows-1252", "utf-8", $windows1252);
echo "converted: " . ($utf8 ?? "conversion failed") . "\n";

echo "stdout VT100: "
    . (sapi_windows_vt100_support(STDOUT) ? "enabled" : "disabled or redirected")
    . "\n";

$handler = function (int $event): void {
    echo $event === 0 ? "Ctrl-C\n" : "Ctrl-Break\n";
};
echo sapi_windows_set_ctrl_handler($handler) ? "handler installed\n" : "handler failed\n";
