#!/usr/bin/env python3
#
# Lists all package versions.
#
from collections import defaultdict
import os
from typing import Any, DefaultDict, Dict, Set
import toml


def main():
    packages_versions: DefaultDict[str, Set[str]] = defaultdict(set)
    for dirpath, _dirnames, filenames in os.walk("."):
        if "Cargo.toml" not in filenames:
            continue

        path = os.path.join(dirpath, "Cargo.toml")
        with open(path, "r") as f:
            cargo = toml.load(f)

        all_deps: Dict[str, Any] = {}
        all_deps.update(cargo.get("dependencies", {}))
        all_deps.update(cargo.get("dev-dependencies", {}))

        for package, value in all_deps.items():
            if isinstance(value, str):
                packages_versions[package].add(value)
                continue

            ver = value.get("version", None)
            if ver is not None:
                packages_versions[package].add(ver)

    for package, versions in sorted(packages_versions.items(), key=lambda kv: kv[0]):
        sorted_versions = ", ".join(sorted(versions))
        print(f"{package}: {sorted_versions}")


if __name__ == "__main__":
    main()
