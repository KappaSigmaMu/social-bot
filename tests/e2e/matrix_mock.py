from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import time
import urllib.parse

ROOM_ID = "!society:e2e.local"
BOT_USER_ID = "@societybot:e2e.local"

state = {
    "next_batch": 0,
    "events": [],
    "sent": [],
}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/_test/health":
            self.json_response({"ok": True})
            return

        if parsed.path == "/_test/messages":
            self.json_response({"messages": state["sent"]})
            return

        if parsed.path == "/_matrix/client/v3/sync":
            query = urllib.parse.parse_qs(parsed.query)
            since = query.get("since", [None])[0]
            events = []
            if since is not None:
                events = state["events"]
                state["events"] = []
            state["next_batch"] += 1
            self.json_response(
                {
                    "next_batch": f"s{state['next_batch']}",
                    "rooms": {
                        "join": {
                            ROOM_ID: {
                                "timeline": {
                                    "events": events,
                                    "limited": False,
                                    "prev_batch": "p0",
                                }
                            }
                        }
                    },
                }
            )
            return

        self.not_found()

    def do_POST(self):
        if self.path == "/_test/events":
            payload = self.read_json()
            state["events"].append(
                {
                    "type": "m.room.message",
                    "sender": payload.get("sender", "@tester:e2e.local"),
                    "origin_server_ts": int(time.time() * 1000),
                    "content": {
                        "msgtype": "m.text",
                        "body": payload["body"],
                    },
                }
            )
            self.json_response({"queued": True})
            return

        self.not_found()

    def do_PUT(self):
        parsed = urllib.parse.urlparse(self.path)
        prefix = f"/_matrix/client/v3/rooms/{urllib.parse.quote(ROOM_ID, safe='')}/send/m.room.message/"
        if parsed.path.startswith(prefix):
            payload = self.read_json()
            state["sent"].append(
                {
                    "sender": BOT_USER_ID,
                    "body": payload.get("body", ""),
                    "content": payload,
                }
            )
            self.json_response({"event_id": f"$e2e-{len(state['sent'])}"})
            return

        self.not_found()

    def read_json(self):
        length = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(length) if length else b"{}"
        return json.loads(body.decode("utf-8"))

    def json_response(self, payload, status=200):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def not_found(self):
        self.json_response({"error": "not found", "path": self.path}, status=404)

    def log_message(self, fmt, *args):
        print(f"[matrix-mock] {self.address_string()} {fmt % args}", flush=True)


if __name__ == "__main__":
    server = ThreadingHTTPServer(("0.0.0.0", 8008), Handler)
    print("[matrix-mock] listening on :8008", flush=True)
    server.serve_forever()
