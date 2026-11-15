#!/usr/bin/env python3
"""Forward the wallet's RPC bytes through the environment's configured HTTPS proxy.

Use only when the SDK's direct HTTP client cannot use the configured proxy.
No JSON-RPC results, transactions, proofs, or fee declarations are modified.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import urllib.request
import urllib.error

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--port", type=int, default=4091)
args = parser.parse_args()
UPSTREAM = "https://testnet.lez.logos.co"


class Relay(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        if self.path != "/" or not 0 < length <= 8 * 1024 * 1024:
            self.send_error(400)
            return
        body = self.rfile.read(length)
        request = urllib.request.Request(UPSTREAM, body, headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                data, status = response.read(), response.status
        except urllib.error.HTTPError as error:
            data, status = error.read(), error.code
        except OSError:
            self.send_error(502, "Official RPC upstream unavailable")
            return
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass  # Never log wallet requests or proof payloads.


ThreadingHTTPServer(("127.0.0.1", args.port), Relay).serve_forever()
