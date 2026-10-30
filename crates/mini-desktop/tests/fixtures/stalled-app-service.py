#!/usr/bin/env python3
"""IPC fault fixture: partial response, then no progress. No app data or network."""
import struct
import sys


def exact(count):
    data = bytearray()
    while len(data) < count:
        part = sys.stdin.buffer.read(count - len(data))
        if not part:
            return None
        data.extend(part)
    return bytes(data)


header = exact(4)
if header is not None:
    size = struct.unpack(">I", header)[0]
    if size < 11 or size > 1024 * 1024 or exact(size) is None:
        raise SystemExit(2)
    # Declare a legal frame, then stall mid-frame to exercise reader cleanup.
    sys.stdout.buffer.write(struct.pack(">I", 16) + b"\x00\x01")
    sys.stdout.buffer.flush()
    while exact(1) is not None:
        pass
