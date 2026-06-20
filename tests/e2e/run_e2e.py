import json
import os
import sys
import time
import urllib.request

BASE_URL = os.environ.get("MATRIX_TEST_URL", "http://127.0.0.1:8008")
TEST_USER = "@tester:e2e.local"
KNOWN_KUSAMA_ADDRESS = "FUfBKr2pDxKrxmExGp4hjU6St4BDgffzKcyAqv6pruGnez1"


def request(method, path, payload=None):
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["content-type"] = "application/json"
    req = urllib.request.Request(BASE_URL + path, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=5) as response:
        return json.loads(response.read().decode("utf-8"))


def messages():
    return request("GET", "/_test/messages")["messages"]


def wait_for_bot_initial_sync():
    deadline = time.time() + 90
    while time.time() < deadline:
        state = request("GET", "/_test/state")
        # The bot intentionally ignores the first sync response so it does not
        # answer stale room history. New test events are safe after this point.
        if state["sync_count"] >= 1:
            return
        time.sleep(1)
    raise AssertionError("bot did not complete initial Matrix sync")


def send_command(body):
    before = len(messages())
    request("POST", "/_test/events", {"body": body, "sender": TEST_USER})
    return before


def wait_for_new_response(after_count, predicate, label):
    deadline = time.time() + 180
    last_messages = []
    while time.time() < deadline:
        current = messages()
        last_messages = current
        for message in current[after_count:]:
            body = message["body"]
            if body.startswith("Error:"):
                raise AssertionError(f"{label}: bot returned error: {body}")
            if predicate(body):
                print(f"[ok] {label}: {body.splitlines()[0]}", flush=True)
                return body
        time.sleep(2)
    raise AssertionError(f"{label}: timed out; messages={last_messages!r}")


def assert_command(label, command, predicate):
    offset = send_command(command)
    return wait_for_new_response(offset, predicate, label)


def assert_no_response(label, body):
    offset = send_command(body)
    time.sleep(5)
    current = messages()
    if len(current) != offset:
        raise AssertionError(f"{label}: expected no bot response; new={current[offset:]!r}")
    print(f"[ok] {label}: no response", flush=True)


def contains_all(*fragments):
    return lambda body: all(fragment in body for fragment in fragments)


def main():
    request("GET", "/_test/health")
    request("POST", "/_test/reset")
    wait_for_bot_initial_sync()
    assert_no_response("non-command Matrix message", "hello bot")

    cases = [
        (
            "ping command",
            "!ping",
            lambda body: body.startswith("Pong!"),
        ),
        (
            "head query through Chopsticks",
            "!head",
            lambda body: body.startswith("The current head is `")
            or body == "There is no head, something must have gone horribly wrong",
        ),
        (
            "period summary through Chopsticks",
            "!period",
            contains_all("We are currently in the", "challenge period"),
        ),
        (
            "defender query through Chopsticks",
            "!defender",
            lambda body: "current defender" in body or body == "There is no defender",
        ),
        (
            "skeptics query through Chopsticks",
            "!skeptics",
            contains_all("skeptic"),
        ),
        (
            "candidate list through Chopsticks",
            "!candidates",
            lambda body: "candidate" in body.lower(),
        ),
        (
            "candidate miss through Chopsticks",
            f"!candidates {KNOWN_KUSAMA_ADDRESS}",
            contains_all("No candidate with address", KNOWN_KUSAMA_ADDRESS),
        ),
        (
            "member info through Chopsticks",
            f"!info {KNOWN_KUSAMA_ADDRESS}",
            contains_all("* **Address**:", "* **State**:"),
        ),
        (
            "address override write",
            f"!set_address {KNOWN_KUSAMA_ADDRESS}",
            contains_all("Set matrix handle", KNOWN_KUSAMA_ADDRESS),
        ),
        (
            "address override read",
            "!me",
            contains_all("* **Address**:", KNOWN_KUSAMA_ADDRESS),
        ),
        (
            "address override delete",
            "!unset_address",
            contains_all("Unset address"),
        ),
    ]

    for label, command, predicate in cases:
        assert_command(label, command, predicate)

    print("e2e passed")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"e2e failed: {error}", file=sys.stderr)
        sys.exit(1)
