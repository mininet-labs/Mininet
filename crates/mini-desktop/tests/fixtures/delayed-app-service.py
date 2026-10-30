#!/usr/bin/env python3
"""IPC-only test peer: require two requests before replying in reverse order.

No identities, networking, files or application state. A serial request/reply
client cannot complete this fixture's first batch.
"""
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


def reply(request_id, shutdown=False):
    # Response v1, request id, Ok, Ack or locked/missing AccountStatus.
    body = struct.pack(">HQBB", 1, request_id, 0, 0 if shutdown else 1)
    if not shutdown:
        body += bytes([0, 0, 0, 0])
    sys.stdout.buffer.write(struct.pack(">I", len(body)) + body)
    sys.stdout.buffer.flush()


pending = []
first_batch = True
while True:
    header = exact(4)
    if header is None:
        break
    length = struct.unpack(">I", header)[0]
    if length < 11 or length > 1024 * 1024:
        raise SystemExit(2)
    frame = exact(length)
    if frame is None:
        raise SystemExit(2)
    version, request_id, command = struct.unpack(">HQB", frame[:11])
    if version != 1:
        raise SystemExit(2)
    if command == 9:
        reply(request_id, shutdown=True)
        break
    if first_batch:
        pending.append(request_id)
        if len(pending) < 2:
            continue
        for queued in reversed(pending):
            reply(queued)
        pending.clear()
        first_batch = False
    else:
        reply(request_id)
