<?php
function runtimeBindingOrigin(int $seed): void {
    $source = $seed > 0 ? '$created = 2.5;' : '';
    eval($source);
    try {
        $created = $seed;
        echo 'W';
    } catch (TypeError $error) {
        echo 'T';
    }
    echo gettype($created), '|', $created;
}
runtimeBindingOrigin($argc);
gc_collect_cycles();

echo '|';
function runRuntimeOriginSource(string $source, mixed $input): void {
    eval($source);
}
$originCases = [
    '$value=1; try {$value="bad"; echo "W";} catch(TypeError $error) {echo "T";} echo $value,"|";',
    '$value=2.5; $value=3.5; try {$value=2; echo "W";} catch(TypeError $error) {echo "T";} echo $value,"|";',
    '$value=$input; $value=1; $value="open"; echo $value,"|";',
    '$value=[1]; $value=["ok"]; try {$value=2; echo "W";} catch(TypeError $error) {echo "T";} echo gettype($value),"|";',
    '$value=null; $value=null; try {$value="bad"; echo "W";} catch(TypeError $error) {echo "T";} echo gettype($value),"|";',
    '$value=1; $alias=&$value; unset($value); try {$alias="bad"; echo "W";} catch(TypeError $error) {echo "T";} echo $alias,"|";',
    '$value=1; unset($value); $value="new"; echo $value,"|";',
    '$value="start"; try {$value=1; echo "W";} catch(TypeError $error) {echo "T";} echo $value,"|";',
];
foreach ($originCases as $originSource) {
    runRuntimeOriginSource($originSource, 2.5);
}
gc_collect_cycles();
