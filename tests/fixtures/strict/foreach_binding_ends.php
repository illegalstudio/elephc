<?php
$firstValues = [1, 2];
foreach ($firstValues as &$firstValue) {
    unset($firstValue);
    $firstValue = 'new';
    echo $firstValue, '|';
}
echo implode(',', $firstValues), '|', gettype($firstValue), ':', $firstValue, '|';

$secondValues = [1, 2];
foreach ($secondValues as &$secondValue) {
    if ($secondValue === 1) { unset($secondValue); $secondValue = 'new'; }
    echo gettype($secondValue), ':', $secondValue, '|';
}
echo implode(',', $secondValues), '|';

$thirdValues = [1, 2];
foreach ($thirdValues as &$thirdValue) { echo $thirdValue, '|'; }
echo implode(',', $thirdValues), '|';
