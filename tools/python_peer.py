"""Optional pinned-upstream compatibility peer; no third-party test runner needed."""

import argparse
import pathlib
import sys

BASELINE = "da93f21a06ee5565f7793323ddbdb542f31ec9a2"


def main() -> None:
    """Serve controlled upstream memory or exercise a Rust emulator as a client."""
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("server", "client"))
    parser.add_argument("--reference", type=pathlib.Path, required=True)
    parser.add_argument("--port", type=int, default=0)
    args = parser.parse_args()
    if sys.version_info < (3, 10):
        raise RuntimeError("Compatibility peer requires Python 3.10 or newer")
    sys.path.insert(0, str(args.reference.resolve()))
    from snap7 import Client, Server
    from snap7.type import SrvArea, Block
    from snap7.util import get_int, set_int

    if args.mode == "server":
        server = Server(log=False)
        memory = bytearray(i % 256 for i in range(600))
        server.register_area(SrvArea.DB, 1, memory)
        server.host = "127.0.0.1"
        server.start(0)
        try:
            print(server.server_socket.getsockname()[1], flush=True)
            # Rust closes stdin or writes a line once the scenario completes.
            sys.stdin.readline()
        finally:
            server.stop()
    else:
        client = Client()
        client.connect("127.0.0.1", 0, 1, args.port)
        try:
            assert client.db_read(1, 0, 300) == bytes(i % 256 for i in range(300))
            payload = bytearray(2)
            set_int(payload, 0, -1234)
            client.db_write(1, 10, payload)
            assert get_int(client.db_read(1, 10, 2), 0) == -1234
            assert client.get_block_info(Block.DB, 1).MC7Size == 600
            uploaded, size = client.full_upload(Block.DB, 1)
            assert len(uploaded) == size == 636
            assert client.upload(1) == client.db_read(1, 0, 600)
            client.download(uploaded)
            assert client.upload(1) == client.db_read(1, 0, 600)
            client.plc_stop()
            client.plc_hot_start()
            print("python-client-ok", flush=True)
        finally:
            client.disconnect()


if __name__ == "__main__":
    main()
