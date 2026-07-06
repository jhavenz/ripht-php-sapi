<?php

header('Content-Type: text/plain');

echo 'before';
$first = fastcgi_finish_request();

ob_start();
echo 'after';
$second = fastcgi_finish_request();
$post_finish_buffer = ob_get_clean();

file_put_contents(getenv('RIPHT_FASTCGI_FINISH_RESULT'), json_encode([
    'first' => $first,
    'second' => $second,
    'post_finish_buffer' => $post_finish_buffer,
]));
