<?php
$tokens = PhpToken::tokenize('<?php echo 1;', TOKEN_PARSE);
echo $tokens[1]->getTokenName(), ':', token_name(T_FUNCTION), ':', count(token_get_all('html'));

#[Export]
function ios_tokenizer_link_smoke(): int {
    return 0;
}
