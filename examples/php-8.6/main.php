<?php

// PHP 8.6 tour: readonly property defaults, Time\Duration, and clamp().
//
// Compile with the 8.6 profile so profile-sensitive builtins (trim's form feed,
// array_filter's mode validation) match PHP 8.6:
//     elephc --php-version=8.6 examples/php-8.6/main.php

use Time\Duration;

final readonly class RetryPolicy
{
    public string $name = 'network';
    public int $attempts = 5;

    public function delay(): Duration
    {
        return Duration::fromMilliseconds(100)->multiplyBy(2 ** $this->attempts);
    }
}

$policy = new RetryPolicy();
$delay = $policy->delay();

echo $policy->name, ' after ', $policy->attempts, ' attempts: ', PHP_EOL;
echo $delay->seconds, '.', str_pad((string) $delay->nanoseconds, 9, '0', STR_PAD_LEFT), "s\n";

$oneSecond = Duration::fromSeconds(1);
$halfSecond = $oneSecond->divideBy(2);
$total = $oneSecond->add($halfSecond);

echo 'total: ', $total->seconds, '.', str_pad((string) $total->nanoseconds, 9, '0', STR_PAD_LEFT), "s\n";
var_dump($halfSecond < $oneSecond);

var_dump(clamp(105, 0, 100));
var_dump(clamp('P', 'A', 'Z'));
var_dump(clamp(null, -1, 1));

$trimmed = trim("\x0C \tform feed is whitespace in 8.6\x0C");
echo $trimmed, PHP_EOL;
