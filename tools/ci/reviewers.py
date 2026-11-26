"""The trusted reviewers in .github/trusted-reviewers.txt, who triage issues and pull requests with comments
(tools/ci/triage_command.py) without any repository role."""
from __future__ import annotations

import re
from dataclasses import dataclass

OWNER = "realdoc06"  # lowercase: `is_owner` compares lowercased logins
REVIEWERS = ".github/trusted-reviewers.txt"
LOGIN = re.compile(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})$")


@dataclass
class Reviewers:
    logins: set[str]  # lowercase
    paused: bool


def read_reviewers(text: str) -> Reviewers:
    """The list file: one login per line, `#` comments, and a `paused` line that stops every reviewer."""
    logins, paused = set(), False
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        words = line.split()
        if len(words) != 1:  # not a login: ignored rather than half-read
            continue
        word = words[0]
        if word.lower() == "paused":
            paused = True
        elif LOGIN.match(word):
            logins.add(word.lower())
    return Reviewers(logins, paused)


def is_owner(login: str) -> bool:
    return login.lower() == OWNER
