#!/usr/bin/env python3
import sys
import subprocess
import toml


def main():
    with open("Cargo.toml", "r", encoding="utf-8") as f:
        cargo = toml.load(f)

    workspace_members = cargo.get("workspace", {}).get("members", [])
    failures = []
    for member in workspace_members:
        print(f"Building {member}", file=sys.stderr, flush=True)
        result = subprocess.run(
            ["cargo", "build", "--package", member, "--all-targets"],
        )
        if result.returncode != 0:
            failures.append(member)

    if failures:
        print(f"these packages' builds failed: {failures!r}")
        sys.exit(1)
    else:
        print("all packages built successfully")
        sys.exit(0)


if __name__ == "__main__":
    main()
