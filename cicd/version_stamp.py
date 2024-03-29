#!/usr/bin/env python3
import glob
import os.path
import subprocess
import sys
from typing import List


CODE_FILES = (
    [os.path.join("rocketbot_plugin_version", "src", "lib.rs")]
    + glob.glob(os.path.join("rocketbotweb", "templates", "*.html"))
)
SHORT_HASH_PLACEHOLDER = "{{VERSION}}"
COMMIT_SUBJECT_PLACEHOLDER = "{{COMMIT_MESSAGE_SHORT}}"


def get_output(args: List[str]) -> str:
    completed_process = subprocess.run(
        args,
        capture_output=True,
        check=False,
        text=True,
    )
    if completed_process.returncode != 0:
        sys.stdout.write(completed_process.stdout)
        sys.stderr.write(completed_process.stderr)
        sys.exit(completed_process.returncode)

    return completed_process.stdout.strip()


def rust_escape(s: str) -> str:
    ret = []
    for c in s:
        if c == "\\":
            ret.append("\\\\")
        elif c == '"':
            ret.append('\\"')
        else:
            ret.append(c)
    return "".join(ret)


def main():
    short_hash = get_output(["git", "show", "--pretty=tformat:%h", "--no-patch", "HEAD"])
    commit_subject = get_output(["git", "show", "--pretty=tformat:%s", "--no-patch", "HEAD"])

    for code_file in CODE_FILES:
        with open(code_file, "r", encoding="utf-8") as f:
            code = f.read()

        code = (
            code
                .replace(SHORT_HASH_PLACEHOLDER, rust_escape(short_hash))
                .replace(COMMIT_SUBJECT_PLACEHOLDER, rust_escape(commit_subject))
        )

        with open(code_file, "w", encoding="utf-8") as f:
            f.write(code)


if __name__ == "__main__":
    main()
