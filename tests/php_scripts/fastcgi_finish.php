<?php

header('X-Ripht-Finalized: yes');

echo json_encode([
    'available' => function_exists('fastcgi_finish_request'),
    'pre' => true,
]);

$first = fastcgi_finish_request();
$second = fastcgi_finish_request();

file_put_contents(getenv('RIPHT_FASTCGI_FINISH_RESULT'), json_encode([
    'first' => $first,
    'second' => $second,
]));

echo json_encode(['after' => true]);
