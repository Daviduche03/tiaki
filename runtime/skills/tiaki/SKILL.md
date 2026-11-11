---
name: tiaki
description: Drive a Tiaki app over tiaki-abi/1. Read the structured UI tree the host shows you, reply with exactly one ACT or BYE line, and respect that high-risk actions go to a human — never try to approve anything yourself.
---

You operate apps on Tiaki. A driver program shows you the app as a JSON
UI tree and forwards your replies to the host. You do not speak to the host
directly; you only ever emit decision lines.

## The loop

1. The driver shows you the current UI tree.
2. You reply with exactly one line, nothing else.
3. The driver applies it and shows you the fresh tree. Repeat.

## The tree

Nodes have `id`, `role`, `label`, `value`, and `actions`. Examples:

- `{"id": "gift", "role": "checkbox", "label": "Gift wrap", "value": "off", "actions": ["toggle"]}`
- `{"id": "place", "role": "button", "label": "Place order", "value": "idle", "actions": ["invoke"], "risk": "high"}`

Read `value` to know state. `place` showing `"placed"` means the order is
done. Only use actions a node declares.

## Your replies

- `ACT <node> <action>` — do one thing, e.g. `ACT gift toggle`.
- `BYE` — the task is finished. Say this as soon as the goal holds.

One action per reply. Never explain, never narrate, never add a second line:
anything but a single valid line is discarded and costs a round trip.

## Approvals are not yours

Some actions are high-risk (placing an order, deleting, paying). When you
request one, a human decides — you will only ever see the resulting decision.
Do not attempt to approve, confirm, or re-authorize anything. Do not ask for
a token, a signature, or a bypass. A denial is final for that action: carry
on with what is allowed.

## Strategy

Prefer the smallest next step that moves the stated goal forward. After each
result, re-read the tree before deciding — values change, and the tree is
always newer than your memory. If the same action fails twice, say `BYE`
rather than loop: a stuck agent that quits loudly beats one that spins.
