#!/usr/bin/env python3
"""Preview or delete compiled artifacts by signature, preserving media and fonts."""

# SPDX-License-Identifier: GPL-3.0-only
import argparse
import json
import os
from pathlib import Path
import struct

SKIP = {'.git', '.venv', 'venv', 'node_modules', 'vaults'}

def compiled(path):
    with path.open('rb') as stream:
        header = stream.read(128)
        if header.startswith((b'\x7fELF', b'!<arch>\n', b'BC\xc0\xde', b'\x00asm\x01\x00\x00\x00',
                              b'\xfe\xed\xfa\xce', b'\xce\xfa\xed\xfe', b'\xfe\xed\xfa\xcf',
                              b'\xcf\xfa\xed\xfe', b'\xca\xfe\xba\xbe', b'\xbe\xba\xfe\xca',
                              b'\xca\xfe\xba\xbf', b'\xbf\xba\xfe\xca', b'\xed\xab\xee\xdb',
                              b'Microsoft C/C++ MSF 7.00\r\n\x1aDS\x00\x00\x00')):
            return True
        if header.startswith(b'MZ') and len(header) >= 64:
            offset = struct.unpack_from('<I', header, 60)[0]
            if 64 <= offset <= 16 * 1024 * 1024:
                stream.seek(offset)
                return stream.read(4) == b'PE\x00\x00'
        if path.suffix.lower() in {'.o', '.obj'} and len(header) >= 20:
            machine, sections = struct.unpack_from('<HH', header)
            if machine in {0x14c, 0x8664, 0xaa64} and 0 < sections <= 96:
                return True
        if path.suffix.lower() in {'.pyc', '.pyo'} and len(header) >= 16 and header[2:4] == b'\r\n':
            return True
        if path.suffix.lower() == '.rmeta' and header.startswith(b'rust') and b'\x00' in header:
            return True
        if path.suffix.lower() in {'.jar', '.war', '.ear', '.apk', '.aab'} and header.startswith(b'PK\x03\x04'):
            return True
        if '.pkg.tar.' in path.name and header.startswith((b'\x28\xb5\x2f\xfd', b'\x1f\x8b', b'\xfd7zXZ\x00')):
            return True
    return False

def clean(root, delete=False):
    count = 0
    failures = 0
    for directory, directories, files in os.walk(root, followlinks=False):
        directories[:] = sorted(name for name in directories
                                if name not in SKIP and not (Path(directory) / name).is_symlink())
        for name in sorted(files):
            path = Path(directory) / name
            try:
                if path.is_symlink() or not path.is_file():
                    continue
                identity = path.stat(follow_symlinks=False)
                if not compiled(path):
                    continue
                label = json.dumps(path.relative_to(root).as_posix(), ensure_ascii=False)
                if delete:
                    current = path.stat(follow_symlinks=False)
                    if path.is_symlink() or (current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns) != (
                            identity.st_dev, identity.st_ino, identity.st_size, identity.st_mtime_ns):
                        raise OSError('file changed during inspection; skipped')
                    path.unlink()
                print(('Deleted ' if delete else 'Would delete ') + label)
                count += 1
            except OSError as error:
                print(f'Skipped {json.dumps(str(path.relative_to(root)))}: {error}')
                failures += 1
    print(f'{count} compiled files {"deleted" if delete else "found"}; {failures} errors.')
    if not delete:
        print('Preview only. Run again with --delete to remove these compiled files.')
    return failures

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--delete', action='store_true', help='delete detected compiled files (default: preview)')
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1], help='Diamond source directory')
    args = parser.parse_args()
    root = args.root.resolve()
    if not (root / 'Cargo.toml').is_file() or not (root / 'crates').is_dir():
        parser.error('--root must contain Diamond Cargo.toml and crates/')
    return bool(clean(root, args.delete))

if __name__ == '__main__':
    raise SystemExit(main())
