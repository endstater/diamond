#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Protocol v2 integration fixture; no PDF/Typst renderer or external packages."""
import base64
import json
import struct
import sys
import zlib


def png_page(line_count):
    width, height = 480, 640
    pixels = bytearray()
    for y in range(height):
        pixels.append(0)
        for x in range(width):
            text_line = 40 <= x < 420 and 32 <= y < min(600, 32 + line_count * 20) and (y - 32) % 20 < 5
            pixels.extend((85, 95, 120) if text_line else (246, 246, 246))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)

    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(pixels)) + chunk(b'IEND', b'')


def handle(method, params):
    if method == 'initialize':
        return {'protocol_version': 2}
    if method == 'commands/list':
        return {'commands': []}
    if method == 'views/list':
        return {'views': [{'id': 'lines', 'title': 'Native preview demo', 'extensions': ['previewdemo'], 'editable': True}]}
    if method == 'views/update':
        document = params['document']
        text = document.get('text') or ''
        offsets = [0]
        for line in text.splitlines(keepends=True):
            offsets.append(offsets[-1] + len(line))
        anchors = [{'offset': offset, 'page': 0, 'x': 0.1, 'y': min(0.93, (32 + row * 20) / 640)} for row, offset in enumerate(offsets[:29])]
        result = {'revision': document['revision'], 'pages': [{'id': 'page-0', 'png_base64': base64.b64encode(png_page(len(offsets))).decode('ascii')}], 'anchors': anchors}
        event = params.get('event') or {}
        if event.get('kind') in ('click', 'scroll'):
            result['cursor'] = min(anchors, key=lambda anchor: abs(anchor['y'] - event['y']))['offset']
        return result
    raise ValueError('Unknown method')


if __name__ == '__main__':
    for line in sys.stdin:
        request = json.loads(line)
        try:
            reply = {'result': handle(request['method'], request.get('params', {}))}
        except (KeyError, TypeError, ValueError) as error:
            reply = {'error': {'code': -32602, 'message': str(error)}}
        print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], **reply}), flush=True)
