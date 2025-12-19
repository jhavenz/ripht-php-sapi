<?php

$code = (int)($_GET['code'] ?? 200);
$method = $_GET['method'] ?? 'code';

switch ($method) {
    case 'code':
        http_response_code($code);
        break;

    case 'header':
        $reasons = [
            100 => 'Continue',
            101 => 'Switching Protocols',
            200 => 'OK',
            201 => 'Created',
            202 => 'Accepted',
            204 => 'No Content',
            301 => 'Moved Permanently',
            302 => 'Found',
            303 => 'See Other',
            304 => 'Not Modified',
            307 => 'Temporary Redirect',
            308 => 'Permanent Redirect',
            400 => 'Bad Request',
            401 => 'Unauthorized',
            403 => 'Forbidden',
            404 => 'Not Found',
            405 => 'Method Not Allowed',
            409 => 'Conflict',
            410 => 'Gone',
            418 => "I'm a teapot",
            422 => 'Unprocessable Entity',
            429 => 'Too Many Requests',
            500 => 'Internal Server Error',
            501 => 'Not Implemented',
            502 => 'Bad Gateway',
            503 => 'Service Unavailable',
            504 => 'Gateway Timeout',
        ];
        $reason = $reasons[$code] ?? 'Unknown';
        header("HTTP/1.1 $code $reason");
        break;

    case 'both':
        http_response_code($code);
        header("X-Original-Code: $code");
        break;
}

if ($code !== 204 && $code !== 304) {
    header('Content-Type: application/json');
    echo json_encode([
        'requested_code' => $code,
        'method' => $method,
        'actual_code' => http_response_code(),
    ], JSON_PRETTY_PRINT);
}
