<?php

ob_start(static function (string $body): string {
    return "{$body}|final";
});

echo 'before';

$finished = fastcgi_finish_request();

file_put_contents(getenv('RIPHT_FASTCGI_FINAL_HANDLER_PATH'), json_encode([
    'finished' => $finished,
    'marker' => 'final-handler-complete',
]));

echo '|after';
