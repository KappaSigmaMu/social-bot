import json
import os
import sys
import time
import urllib.request

BASE_URL = os.environ.get("MATRIX_TEST_URL", "http://127.0.0.1:8008")


def request(method, path, payload=None):
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["content-type"] = "application/json"
    req = urllib.request.Request(BASE_URL + path, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=5) as response:
        return json.loads(response.read().decode("utf-8"))


def wait_for_bot_initial_sync():
    deadline = time.time() + 60
    while time.time() < deadline:
        # The first bot sync consumes no events. Queueing a harmless non-command lets
        # us observe that the bot is at least polling the mock homeserver.
        request("POST", "/_test/events", {"body": "hello"})
        time.sleep(1)
        return
    raise AssertionError("bot did not reach initial Matrix sync")


def wait_for_response(expected_fragment):
    deadline = time.time() + 180
    last_messages = []
    while time.time() < deadline:
        messages = request("GET", "/_test/messages")["messages"]
        last_messages = messages
        for message in messages:
            body = message["body"]
            if expected_fragment in body:
                return body
            if body.startswith("Error:"):
                raise AssertionError(f"bot returned error: {body}")
        time.sleep(2)
    raise AssertionError(f"timed out waiting for {expected_fragment!r}; messages={last_messages!r}")


def main():
    request("GET", "/_test/health")
    wait_for_bot_initial_sync()
    request("POST", "/_test/events", {"body": "!head", "sender": "@tester:e2e.local"})
    body = wait_for_response("current head")
    print("e2e passed; bot response:")
    print(body)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"e2e failed: {error}", file=sys.stderr)
        sys.exit(1)
