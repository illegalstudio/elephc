<?php

enum EnumArrayMode: string
{
    case Fast = 'fast';
    case Safe = 'safe';
}

class EnumArrayDefaults
{
    public const VALUES = [
        EnumArrayMode::Fast,
        ['choice' => EnumArrayMode::Safe],
        'repeat' => EnumArrayMode::Fast,
    ];

    public function values(array $values = self::VALUES): void
    {
    }
}

$name = $argv[1] ?? EnumArrayDefaults::class;
$reflection = new ReflectionClass($name);
$reports = [
    $reflection->getConstant('VALUES'),
    $reflection->getReflectionConstant('VALUES')->getValue(),
    $reflection->getMethod('values')->getParameters()[0]->getDefaultValue(),
];
foreach ($reports as $values) {
    echo $values[0] === EnumArrayMode::Fast ? 'same:' : 'wrong:';
    echo $values[1]['choice']->value, ':';
    echo $values['repeat'] === EnumArrayMode::Fast ? 'same' : 'wrong';
    echo "\n";
}
$reports[0][1]['choice'] = EnumArrayMode::Fast;
echo $reports[1][1]['choice']->value, ':', $reports[2][1]['choice']->value, "\n";
