<?php

class BuildMetadata
{
    public string $channel = 'stable';
    private string $token = 'not exported';
}

gc_enable();

$formats = ['DATE_ATOM', 'DATE_RFC7231'];
foreach ($formats as $format) {
    echo $format, ': ', constant($format), PHP_EOL;
}

$metadata = get_object_vars(new BuildMetadata());
echo 'Visible fields: ', sizeof($metadata), PHP_EOL;
echo 'Random upper bound: ', getrandmax(), PHP_EOL;

$dateFunctions = get_extension_funcs('date');
if ($dateFunctions === false) {
    echo 'Date extension is unavailable', PHP_EOL;
    exit(1);
}
echo 'Date extension functions: ', count($dateFunctions), PHP_EOL;
echo 'First/last: ', $dateFunctions[0], ' / ', end($dateFunctions), PHP_EOL;
