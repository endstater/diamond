#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Exercise actual v2 JSON-RPC fixture, PNG output, scalar maps and click navigation."""
import base64
import json
from pathlib import Path
import struct
import subprocess

root=Path(__file__).resolve().parents[1]
program=root/'examples/plugins/native-preview/plugin.py'
text='я🙂\n你好\n'
requests=[
    ('initialize',{'protocol_version':2,'capabilities':['document_views','document_read']}),
    ('commands/list',{}),('views/list',{}),
    ('views/update',{'document':{'view':'lines','document_id':'test','revision':9,'file_path':'/never-read.previewdemo','text':text},'event':None}),
    ('views/update',{'document':{'view':'lines','document_id':'test','revision':9,'file_path':'/never-read.previewdemo','text':text},'event':{'kind':'click','page':0,'x':0.1,'y':52/640}}),
]
wire=''.join(json.dumps({'jsonrpc':'2.0','id':index,'method':method,'params':params})+'\n' for index,(method,params) in enumerate(requests))
process=subprocess.run(['python3',str(program)],input=wire,text=True,capture_output=True,timeout=5,check=True)
replies=[json.loads(line) for line in process.stdout.splitlines()]
assert len(replies)==len(requests)
for index,reply in enumerate(replies):assert reply['id']==index and reply['jsonrpc']=='2.0' and 'error' not in reply
assert replies[0]['result']['protocol_version']==2
assert replies[2]['result']['views'][0]['editable'] is True
frame=replies[3]['result'];assert frame['revision']==9
assert frame['anchors'][1]['offset']==3
assert all(anchor['offset']<=len(text) for anchor in frame['anchors'])
page=base64.b64decode(frame['pages'][0]['png_base64'],validate=True)
assert page.startswith(b'\x89PNG\r\n\x1a\n')
assert struct.unpack('>II',page[16:24])==(480,640)
assert replies[4]['result']['cursor']==3
print('PASS: protocol v2 fixture, native PNG, Unicode scalar anchors, click navigation')
