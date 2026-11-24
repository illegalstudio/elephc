<?php
// A consumer-like worker that sends its transactions to a listening collector:
// it handles jobs in a loop, each one a transaction (--apm-transaction
// handle_job), and some of them fail. demo.sh builds two versions of it and runs
// several instances, so the monitor page has scripts, apps and versions to show.
//
//   JOBS        how many jobs this run handles (default 20)
//   FAIL_EVERY  one job in FAIL_EVERY throws (0: none)
//   EXIT_CODE   the code the run exits with (default 0)

function handle_job(int $job): int {
    $sum = 0;
    for ($i = 0; $i < 20000; $i++) {
        $sum += crc32((string) ($job * $i)) % 13;
    }
    $every = (int) (getenv('FAIL_EVERY') ?: 0);
    if ($every > 0 && $job % $every === 0) {
        throw new RuntimeException("job $job failed");
    }
    return $sum;
}

$jobs = (int) (getenv('JOBS') ?: 20);
$failed = 0;
for ($job = 1; $job <= $jobs; $job++) {
    try {
        handle_job($job);
    } catch (RuntimeException $e) {
        $failed++;
    }
    usleep(200000);
}
echo "handled $jobs jobs, $failed failed\n";
exit((int) (getenv('EXIT_CODE') ?: 0));
