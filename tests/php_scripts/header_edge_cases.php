<?php

$test = $_GET['test'] ?? 'basic';

switch ($test) {
    case 'duplicate':
        header('Set-Cookie: a=1; Path=/');
        header('Set-Cookie: b=2; Path=/; HttpOnly', false);
        header('Set-Cookie: c=3; Path=/; Secure', false);
        break;

    case 'multivalue':
        header('X-Custom: value1');
        header('X-Custom: value2', false);
        header('X-Custom: value3', false);
        break;

    case 'cachecontrol':
        header('Cache-Control: no-cache, no-store, must-revalidate');
        header('Pragma: no-cache');
        header('Expires: 0');
        break;

    case 'longvalue':
        $long = str_repeat('x', 8000);
        header('X-Long-Header: ' . $long);
        break;

    case 'specialchars':
        header("X-Tab-Header: before\tafter");
        header("X-Space-Header:  multiple   spaces  ");
        break;

    case 'remove':
        header('X-To-Remove: value');
        header_remove('X-To-Remove');
        header('X-Kept: still here');
        break;

    case 'replace_status':
        http_response_code(201);
        header('HTTP/1.1 204 No Content');
        break;

    case 'content_disposition':
        header('Content-Type: application/octet-stream');
        header('Content-Disposition: attachment; filename="test file (1).txt"');
        echo "file content";
        exit;

    default:
        header('Content-Type: application/json');
}

echo json_encode([
    'test' => $test,
    'headers_list' => headers_list(),
    'http_response_code' => http_response_code(),
], JSON_PRETTY_PRINT);
