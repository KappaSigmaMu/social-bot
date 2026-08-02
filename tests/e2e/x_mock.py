from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import threading

state = {"posts": []}
state_lock = threading.Lock()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        if self.path == "/_x/health":
            self.json_response({"ok": True})
            return
        if self.path == "/_x/posts":
            with state_lock:
                posts = list(state["posts"])
            self.json_response({"posts": posts})
            return
        self.json_response({"error": "not found", "path": self.path}, status=404)

    def do_POST(self):
        if self.path == "/webhook":
            payload = self.read_json()
            with state_lock:
                state["posts"].append(payload)
            self.json_response({"ok": True})
            return
        self.json_response({"error": "not found", "path": self.path}, status=404)

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

    def log_message(self, fmt, *args):
        print(f"[x-mock] {self.address_string()} {fmt % args}", flush=True)


if __name__ == "__main__":
    server = ThreadingHTTPServer(("0.0.0.0", 8081), Handler)
    print("[x-mock] listening on :8081", flush=True)
    server.serve_forever()
