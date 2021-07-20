#!/usr/bin/env python3
#
# Verifies that all plugins have been registered where they must be.
#
import glob
import os.path
import sys
import toml


PREFIX = "rocketbot_plugin_"
IF_LINE_SUBSTRING = " if plugin_config.name == "
NEW_LINE_SUBSTRING = "::new(iface_weak, inner_config).await"


def main():
    with open("Cargo.toml", "r", encoding="utf-8") as f:
        top_cargo_config = toml.load(f)

    with open(os.path.join("rocketbot", "Cargo.toml"), "r", encoding="utf-8") as f:
        rocketbot_cargo_config = toml.load(f)

    with open(os.path.join("rocketbot", "src", "plugins.rs"), "r", encoding="utf-8") as f:
        plugin_loader_lines = [
            line
            for line in f.readlines()
            if IF_LINE_SUBSTRING in line
            or NEW_LINE_SUBSTRING in line
        ]

    bad = False
    for plug_full_name in glob.glob(f"{PREFIX}*"):
        plug_name = plug_full_name[len(PREFIX):]

        try:
            with open(os.path.join(plug_full_name, "Cargo.toml"), "r", encoding="utf-8") as f:
                plugin_cargo_config = toml.load(f)
        except FileNotFoundError:
            print(f"{plug_name!r} does not have a Cargo.toml")
            bad = True

        if plugin_cargo_config["package"]["name"] != plug_full_name:
            print(f"{plug_name!r} has an incorrect name (expected {plug_full_name!r}, got {plugin_cargo_config['package']['name']!r}")
            bad = True

        # ensure it is part of the workspace
        if plug_full_name not in top_cargo_config["workspace"]["members"]:
            print(f"{plug_name!r} is missing in the top-level Cargo.toml")
            bad = True

        plug_def = rocketbot_cargo_config["dependencies"].get(plug_full_name, None)
        if plug_def is None:
            print(f"{plug_name!r} is missing as a dependency in the rocketbot Cargo.toml")
            bad = True
        elif plug_def.get("path", "") != f"../{plug_full_name}":
            print(f"{plug_name!r} has an invalid path in the rocketbot Cargo.toml")
            bad = True

        has_if_line = any(
            line for line in plugin_loader_lines
            if IF_LINE_SUBSTRING in line
            and plug_name in line
        )
        if not has_if_line:
            print(f"{plug_name!r} has no \"if\" line in rocketbot::plugins")
            bad = True

        has_new_line = any(
            line for line in plugin_loader_lines
            if NEW_LINE_SUBSTRING in line
            and plug_name in line
        )
        if not has_new_line:
            print(f"{plug_name!r} has no \"new\" line in rocketbot::plugins")
            bad = True

    if bad:
        sys.exit(1)


if __name__ == "__main__":
    main()
