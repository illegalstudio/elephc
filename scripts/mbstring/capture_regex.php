<?php
// Independent PHP 8.5.10 mbregex observations, with every PHP string encoded as hex.
if (PHP_VERSION !== '8.5.10' || MB_ONIGURUMA_VERSION !== '6.9.10') {
    throw new RuntimeException('The mbregex oracle requires PHP 8.5.10 and Oniguruma 6.9.10');
}
while (($line = fgets(STDIN)) !== false) {
    $case = json_decode($line, true, 512, JSON_THROW_ON_ERROR);
    $warnings = [];
    set_error_handler(function ($level, $message) use (&$warnings) { $warnings[] = bin2hex($message); return true; });
    try {
        mb_regex_encoding('UTF-8');
        mb_regex_set_options('pr');
        if ($case['op'] === 'options') {
            mb_regex_set_options(hex2bin($case['value']));
            $result = mb_regex_set_options();
        } elseif ($case['op'] === 'encoding') {
            mb_regex_encoding(hex2bin($case['value']));
            $result = mb_regex_encoding();
        } else {
            mb_regex_encoding($case['encoding']);
            mb_regex_set_options($case['options']);
            $pattern = hex2bin($case['pattern']);
            $subject = hex2bin($case['subject']);
            $regs = [];
            $matched = $case['op'] === 'match' ? mb_ereg_match($pattern, $subject) : mb_ereg($pattern, $subject, $regs);
            $groups = [];
            foreach ($regs as $key => $value) {
                $groups[] = [is_int($key) ? $key : bin2hex($key), $value === false ? false : bin2hex($value)];
            }
            $result = [$matched, $groups];
        }
        $case['result'] = $result;
    } catch (Throwable $error) {
        $case['error'] = [get_class($error), bin2hex($error->getMessage())];
    } finally {
        restore_error_handler();
    }
    $case['warnings'] = $warnings;
    echo json_encode($case, JSON_THROW_ON_ERROR), "\n";
}
