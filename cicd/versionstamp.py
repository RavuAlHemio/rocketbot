#!/usr/bin/env python3
import os.path
import subprocess
import sys


CODE_FILE = os.path.join("rocketbot_plugin_version", "src", "lib.rs")
SHORT_HASH_PLACEHOLDER = "{{VERSION}}"
COMMIT_SUBJECT_PLACEHOLDER = "{{COMMIT_MESSAGE_SHORT}}"


def get_output(args: list[str]) -> str:
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


def main():
    short_hash = get_output(["git", "show", "--pretty=tformat:%h", "--no-patch", "HEAD"])
    commit_subject = get_output(["git", "show", "--pretty=tformat:%s", "--no-patch", "HEAD"])

    with open(CODE_FILE, "r", encoding="utf-8") as f:
        code = f.read()

    code = (
        code
            .replace(SHORT_HASH_PLACEHOLDER, short_hash)
            .replace(COMMIT_SUBJECT_PLACEHOLDER, commit_subject)
    )

    with open(CODE_FILE, "w", encoding="utf-8") as f:
        f.write(code)


if __name__ == "__main__":
    main()
