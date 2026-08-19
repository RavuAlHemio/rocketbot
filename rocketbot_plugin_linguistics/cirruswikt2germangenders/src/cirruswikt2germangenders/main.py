import bz2
import io
import json
import sys
import tomllib
from typing import Mapping

import mwparserfromhell

from . import gender_parsing, wikitext
from .progress import ByteIoProgressWrapper


def read_to_newline(bio: io.BufferedIOBase) -> bytearray:
    buffer = bytearray()
    while not buffer.endswith(b"\n"):
        buffer.append(0x00)
        now_read = bio.readinto(memoryview(buffer)[-1:])
        if now_read == 0:
            # EOF
            buffer.pop()
            return buffer
    return buffer

def process_json_lines(bz2_buffer: io.BufferedIOBase, override_dict: Mapping[str, str]) -> None:
    while (json_line_bs := read_to_newline(bz2_buffer)):
        json_line_str = json_line_bs.decode("utf-8")
        json_line = json.loads(json_line_str)

        title = json_line.get("title", None)
        if title is None:
            continue

        override_value = override_dict.get(title, None)
        if override_value is not None:
            gender = gender_parsing.parse_complete_gender_spec(override_value)
            print(title, repr(gender))
            continue

        source_text = json_line.get("source_text", None)
        if source_text is None:
            continue

        categories = json_line["category"]
        if "German non-lemma forms" in categories:
            # never mind
            continue

        wikitext.process_page(title, source_text)

def main():
    if len(sys.argv) == 1:
        config_path = "config.toml"
    elif len(sys.argv) == 2:
        config_path = sys.argv[1]
    else:
        print(f"Usage: {sys.argv[0]} [CONFIG.TOML]", file=sys.stderr)
        sys.exit(1)

    with open(config_path, "rb") as f:
        config = tomllib.load(f)

    override_dict: dict[str, str] = {
        override_obj["key"]: override_obj["gender"]
        for override_obj
        in config.get("overrides", [])
    }

    for content_json_bz2_path in config["content_json_bz2_paths"]:
        print(f"{content_json_bz2_path}", file=sys.stderr)
        with open(content_json_bz2_path, "rb") as bz2_file:
            with ByteIoProgressWrapper(bz2_file) as bz2_progress:
                with bz2.BZ2File(bz2_progress, "r") as bz2_decompressor:
                    with io.BufferedReader(bz2_decompressor) as bz2_buffer:
                        process_json_lines(bz2_buffer, override_dict)

if __name__ == "__main__":
    main()
