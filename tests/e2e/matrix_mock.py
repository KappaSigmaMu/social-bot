from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import threading
import time
import urllib.parse

ROOM_ID = "!society:e2e.local"
BOT_USER_ID = "@societybot:e2e.local"

state = {
    "next_batch": 0,
    "sync_count": 0,
    "events": [],
    "sent": [],
}
state_lock = threading.Lock()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/_test/health":
            self.json_response({"ok": True})
            return

        if parsed.path == "/_test/messages":
            with state_lock:
                messages = list(state["sent"])
            self.json_response({"messages": messages})
            return

        if parsed.path == "/_test/state":
            with state_lock:
                payload = {
                    "next_batch": state["next_batch"],
                    "sync_count": state["sync_count"],
                    "queued_events": len(state["events"]),
                    "sent_messages": len(state["sent"]),
                }
            self.json_response(
                payload
            )
            return

        if parsed.path == "/_matrix/client/v3/sync":
            query = urllib.parse.parse_qs(parsed.query)
            since = query.get("since", [None])[0]
            if since is not None:
                deadline = time.time() + 0.5
                while time.time() < deadline:
                    with state_lock:
                        if state["events"]:
                            break
                    time.sleep(0.05)
            with state_lock:
                events = []
                if since is not None:
                    events = list(state["events"])
                    state["events"] = []
                state["next_batch"] += 1
                state["sync_count"] += 1
                next_batch = f"s{state['next_batch']}"
            self.json_response(
                {
                    "next_batch": next_batch,
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
        if self.path == "/_test/reset":
            with state_lock:
                state["events"] = []
                state["sent"] = []
            self.json_response({"reset": True})
            return

        if self.path == "/_test/events":
            payload = self.read_json()
            with state_lock:
                event_id = f"$cmd-{len(state['events'])}"
                state["events"].append(
                    {
                        "type": "m.room.message",
                        "event_id": event_id,
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
            with state_lock:
                state["sent"].append(
                    {
                        "sender": BOT_USER_ID,
                        "body": payload.get("body", ""),
                        "content": payload,
                    }
                )
                event_id = f"$e2e-{len(state['sent'])}"
            self.json_response({"event_id": event_id})
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
        if self.path.startswith("/_matrix/client/v3/sync"):
            return
        print(f"[matrix-mock] {self.address_string()} {fmt % args}", flush=True)


if __name__ == "__main__":
    server = ThreadingHTTPServer(("0.0.0.0", 8008), Handler)
    print("[matrix-mock] listening on :8008", flush=True)
    server.serve_forever()
