<?php

header('Content-Type: application/json; charset=utf-8');

$utf8_string = "Hello \xC3\xA9\xC3\xA0\xC3\xBC \xE4\xB8\xAD\xE6\x96\x87";
$emoji = "\xF0\x9F\x98\x80\xF0\x9F\x8E\x89\xF0\x9F\x94\xA5";
$special = "Tab:\tNewline:\nCarriage:\rBackslash:\\Quote:\"";
$cyrillic = "\xD0\x9F\xD1\x80\xD0\xB8\xD0\xB2\xD0\xB5\xD1\x82";
$arabic = "\xD9\x85\xD8\xB1\xD8\xAD\xD8\xA8\xD8\xA7";

$mixed_query = $_GET['data'] ?? '';

echo json_encode([
    'utf8' => $utf8_string,
    'emoji' => $emoji,
    'special' => $special,
    'cyrillic' => $cyrillic,
    'arabic' => $arabic,
    'received_query' => $mixed_query,
    'strlen_utf8' => strlen($utf8_string),
    'mb_strlen_utf8' => mb_strlen($utf8_string, 'UTF-8'),
], JSON_PRETTY_PRINT | JSON_UNESCAPED_UNICODE);
