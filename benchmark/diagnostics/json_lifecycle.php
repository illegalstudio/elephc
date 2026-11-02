<?php
// Separate JSON parsing, value release and optional allocator-cache draining.
function jsonLifecycle(string $source, int $repeats, bool $drainCaches): void {
    $decode = 0;
    $release = 0;
    $drain = 0;
    $count = 0;
    for ($i = 0; $i < $repeats; $i++) {
        $start = hrtime(true);
        $value = json_decode($source, true);
        $count += count($value);
        $decoded = hrtime(true);
        unset($value);
        $released = hrtime(true);
        if ($drainCaches) {
            gc_mem_caches();
        }
        $drained = hrtime(true);
        $decode += $decoded - $start;
        $release += $released - $decoded;
        $drain += $drained - $released;
    }
    echo $count, '|', $decode, ':', $release, ':', $drain;
}
jsonLifecycle((string)file_get_contents($argv[1]), (int)$argv[2], (bool)(int)$argv[3]);
