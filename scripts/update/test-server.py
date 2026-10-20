#!/usr/bin/env python3
"""A stand-in for GitHub's release downloads, for trying /update locally (docs/updater.md).

  python test-server.py <releases dir> <port> [latest stable tag]

<releases dir>/<tag>/ holds each release's files, as publish-update-files.sh
writes them with DRY_RUN. It answers on 127.0.0.1 only, the way github.com
lays releases out:

  /releases/latest/download/<file>      302 to /releases/download/<latest>/<file>
  /releases/download/<tag>/<file>
  /api/git/matching-refs/tags/<prefix>  every <tag> folder starting with <prefix>

Start the client with +set cl_updateURL "http://127.0.0.1:<port>" (the quotes
must reach the game, because // starts a comment).
"""
import http.server
import json
import os
import re
import sys
import urllib.parse

ROOT, PORT = sys.argv[1], int(sys.argv[2])
LATEST = sys.argv[3] if len(sys.argv) > 3 else None


class Handler(http.server.BaseHTTPRequestHandler):
    def send_body(self, status, body, kind="application/octet-stream"):
        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = urllib.parse.unquote(self.path)
        match = re.fullmatch(r"/releases/latest/download/([^/]+)", path)
        if match:
            if not LATEST:
                return self.send_body(404, b"Not Found")
            self.send_response(302)
            self.send_header("Location", f"/releases/download/{LATEST}/{match.group(1)}")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        match = re.fullmatch(r"/releases/download/([^/]+)/([^/]+)", path)
        if match and ".." not in match.groups():
            file = os.path.join(ROOT, match.group(1), match.group(2))
            if os.path.isfile(file):
                with open(file, "rb") as data:
                    return self.send_body(200, data.read())
            return self.send_body(404, b"Not Found")
        match = re.fullmatch(r"/api/git/matching-refs/tags/(.+)", path)
        if match:
            tags = sorted(tag for tag in os.listdir(ROOT) if tag.startswith(match.group(1)))
            body = json.dumps([{"ref": "refs/tags/" + tag} for tag in tags]).encode()
            return self.send_body(200, body, "application/json")
        self.send_body(404, b"Not Found")

    def log_message(self, fmt, *args):
        print(fmt % args, flush=True)


http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
