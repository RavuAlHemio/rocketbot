#!/usr/bin/env python3
import subprocess
import toml


def main():
    with open("Cargo.toml", "r", encoding="utf-8") as f:
        cargo = toml.load(f)

    workspace_members = cargo.get("workspace", {}).get("members", [])
    for member in workspace_members:
        subprocess.run(
            ["cargo", "build", "--package", member, "--all-targets"],
            check=True,
        )


if __name__ == "__main__":
    main()
