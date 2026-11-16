<?php
// A small source inspector that preserves code while removing comments.
$source = '<?php /* explain this */ function greet(string $name): string { return "Hello " . $name; }';
$clean = '';
foreach (PhpToken::tokenize($source, TOKEN_PARSE) as $token) {
    if (!$token->is([T_COMMENT, T_DOC_COMMENT])) {
        $clean .= $token->text;
    }
    if ($token->is(T_FUNCTION)) {
        echo "Function declaration at line ", $token->line, ", byte ", $token->pos, "\n";
    }
}
echo $clean, "\n";
echo 'First token: ', token_name(token_get_all($source)[0][0]), "\n";
