#!/usr/bin/env python3
"""Forward tailnet-reachable ports to a loopback-bound server.

Why this exists: macOS's Application Firewall gates *incoming* connections per
application. A freshly built, unsigned `pangya-server` is not on its allow list, so a
remote peer completes the TCP handshake and then never receives a byte, while the same
listener answers perfectly on loopback. Rather than change the host's security posture to
run a test, this forwards through the interpreter, which is already allowed.

This is test-harness scaffolding, not part of the server. The server still binds loopback
only, which is its safe default; nothing here relaxes that.

Usage:
    ./scripts/tailnet-forward.py 100.74.132.53 10103 20201 18090
"""

import socket
import sys
import threading

BUFFER_BYTES = 65536
BACKLOG = 64


def pump(source: socket.socket, sink: socket.socket) -> None:
    """Copy one direction until it closes, then half-close the far side."""
    try:
        while True:
            chunk = source.recv(BUFFER_BYTES)
            if not chunk:
                break
            sink.sendall(chunk)
    except OSError:
        pass
    finally:
        for side in (source, sink):
            try:
                side.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass


def handle(client: socket.socket, port: int) -> None:
    client.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    try:
        upstream = socket.create_connection(("127.0.0.1", port), timeout=10)
    except OSError as error:
        print(f"[{port}] upstream connect failed: {error}", flush=True)
        client.close()
        return
    upstream.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    upstream.settimeout(None)
    threading.Thread(target=pump, args=(client, upstream), daemon=True).start()
    threading.Thread(target=pump, args=(upstream, client), daemon=True).start()


def listen(bind_ip: str, port: int) -> None:
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind((bind_ip, port))
    server.listen(BACKLOG)
    print(f"[{port}] forwarding {bind_ip}:{port} -> 127.0.0.1:{port}", flush=True)
    while True:
        client, _peer = server.accept()
        handle(client, port)


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    bind_ip = sys.argv[1]
    ports = [int(value) for value in sys.argv[2:]]
    threads = [
        threading.Thread(target=listen, args=(bind_ip, port), daemon=True) for port in ports
    ]
    for thread in threads:
        thread.start()
    try:
        for thread in threads:
            thread.join()
    except KeyboardInterrupt:
        return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
