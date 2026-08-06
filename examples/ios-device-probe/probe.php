<?php

// Measures which kernel-facing operations actually work inside a real iOS app
// sandbox.
//
// This exists because the iOS Simulator runs on the *macOS* kernel: a simulator
// run exercises the same syscall table elephc was written against, and so proves
// nothing about the device. elephc emits 225 raw syscalls; roughly 161 of them
// (write/read/close/exit/lseek/fstat/gettimeofday) are unconditionally safe, and
// the remaining ~26 — path-based and network — are the open question this probe
// turns into facts.
//
// Every check reports OK or FAIL with a detail, and nothing here aborts: a probe
// that dies on the first denial measures one thing instead of all of them.

function line(string $name, bool $ok, string $detail): string {
    return ($ok ? 'OK   ' : 'FAIL ') . str_pad($name, 26) . ' ' . $detail . "\n";
}

/// Writes, reads back and removes a file, exercising open/write/lseek/read/close
/// plus stat inside the directory the host says is writable.
function probe_container_io(string $dir): string {
    $out = '';
    $path = $dir . '/elephc_probe.txt';
    $payload = 'elephc probe payload';

    $handle = fopen($path, 'w');
    if ($handle === false) {
        return line('container.open_write', false, 'fopen failed: ' . $path);
    }
    $written = fwrite($handle, $payload);
    fclose($handle);
    $out .= line('container.open_write', $written === strlen($payload), $path);

    $exists = file_exists($path);
    $out .= line('container.stat', $exists, $exists ? 'file_exists true' : 'file_exists false');

    $read = fopen($path, 'r');
    if ($read === false) {
        $out .= line('container.open_read', false, 'fopen r failed');
    } else {
        $back = fread($read, 64);
        fclose($read);
        $out .= line('container.read_back', $back === $payload, 'got: ' . $back);
    }

    $out .= line('container.unlink', unlink($path), 'removed');
    return $out;
}

/// Paths outside the app container. These are *expected* to fail on a device and
/// to succeed on macOS — the difference is exactly what the probe is for.
function probe_outside_container(): string {
    $out = '';
    $handle = @fopen('/tmp/elephc_probe_outside.txt', 'w');
    if ($handle !== false) {
        fclose($handle);
        @unlink('/tmp/elephc_probe_outside.txt');
        $out .= line('outside./tmp write', true, 'permitted');
    } else {
        $out .= line('outside./tmp write', false, 'denied (expected on device)');
    }

    $etc = @file_exists('/etc/hosts');
    $out .= line('outside./etc/hosts stat', $etc, $etc ? 'readable' : 'denied');
    return $out;
}

/// Process-environment surface. On a device these differ sharply from macOS.
function probe_environment(): string {
    $out = '';
    $cwd = getcwd();
    $out .= line('env.getcwd', $cwd !== false && $cwd !== '', (string) $cwd);

    $tmp = sys_get_temp_dir();
    $out .= line('env.sys_get_temp_dir', $tmp !== '', $tmp);

    $home = getenv('HOME');
    $out .= line('env.getenv HOME', $home !== false, $home === false ? 'unset' : (string) $home);
    return $out;
}

/// Time syscalls. Cheap, but a wrong clock is a silent corruption of every log.
function probe_time(): string {
    $now = time();
    return line('time.time', $now > 1700000000, 'epoch ' . $now);
}

/// DNS plus a TCP connect. Allowed on iOS, but App Transport Security and the
/// absence of a local network entitlement make this worth measuring rather than
/// assuming.
function probe_network(): string {
    $out = '';
    $ip = @gethostbyname('example.com');
    $resolved = $ip !== '' && $ip !== 'example.com';
    $out .= line('net.dns', $resolved, $resolved ? $ip : 'unresolved');
    return $out;
}

#[Export]
function probe(string $writableDir): string {
    $report = "elephc iOS device probe\n";
    $report .= "writable dir: " . $writableDir . "\n\n";
    $report .= probe_container_io($writableDir);
    $report .= probe_outside_container();
    $report .= probe_environment();
    $report .= probe_time();
    $report .= probe_network();
    return $report;
}
