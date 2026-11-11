#!/usr/bin/env python3
"""External checkout plan: the same flow as ScriptedRuntime, but as an OS
process speaking agent-abi/1 over stdio.

Usage:
    echo allow | agentd --principal user:ada computer run -- tests/replay/checkout-plan.py

The script never approves anything itself. When the host raises the approval
challenge, `agentd computer run` asks the operator (here: the piped `allow`
line) and mints a physical input event. Approval the agent cannot mint stays
impossible even with a fully external runtime.
"""
import json
import sys


def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def recv():
    line = sys.stdin.readline()
    if not line:
        raise EOFError("host closed the pipe")
    return json.loads(line)


def main():
    send({
        "kind": "hello", "runtime": "replay-script",
        "protocol": "agent-abi/1", "principal": sys.argv[1] if len(sys.argv) > 1 else "user:ada",
        "session": "ses_replay",
    })
    welcome = recv()
    assert welcome["kind"] == "welcome", welcome

    send({"kind": "observe", "app": "checkout"})
    tree = recv()
    assert tree["kind"] == "tree", tree

    send({"kind": "act", "app": "checkout", "node": "gift", "action": "toggle"})
    acted = recv()
    assert acted.get("ok") is True, acted

    send({"kind": "act", "app": "checkout", "node": "place", "action": "invoke"})
    # The runner holds the challenge itself: it asks the operator and mints
    # the physical event. This pipe carries only the decision, so an external
    # runtime never even sees a spendable challenge id.
    decision = recv()
    assert decision["kind"] == "decision", decision
    assert decision.get("approved") is True, decision

    send({"kind": "observe", "app": "checkout"})
    final_tree = recv()
    assert final_tree["kind"] == "tree", final_tree

    send({"kind": "bye"})


if __name__ == "__main__":
    main()
