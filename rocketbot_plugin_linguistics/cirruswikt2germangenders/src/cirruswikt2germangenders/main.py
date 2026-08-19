import bz2
import io
import json
import sys
import tomllib

import mwparserfromhell

from . import wikitext


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

    for content_json_bz2_path in config["content_json_bz2_paths"]:
        with bz2.BZ2File(content_json_bz2_path, "r") as bz2_file:
            with io.BufferedReader(bz2_file) as bz2_buffer:
                while (json_line_bs := read_to_newline(bz2_buffer)):
                    json_line_str = json_line_bs.decode("utf-8")
                    json_line = json.loads(json_line_str)
                    title = json_line.get("title", None)
                    if title is None:
                        continue
                    source_text = json_line.get("source_text", None)
                    if source_text is None:
                        continue

                    wikitext.process_page(title, source_text)


if __name__ == "__main__":
    main()
