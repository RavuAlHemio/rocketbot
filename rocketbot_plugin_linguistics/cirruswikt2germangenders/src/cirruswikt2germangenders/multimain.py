import bz2
import io
import json
import multiprocessing
import sys
import tomllib
from typing import Iterable, Mapping

from . import wikitext
from .common import GenderFlag, get_override_dict, handle_override_value
from .main import read_to_newline
from .progress import ByteIoProgressWrapper

def process_json_lines(
    process_me_queue: multiprocessing.Queue,
    result_queue: multiprocessing.Queue,
    override_dict: Mapping[str, Iterable[str]],
) -> None:
    while True:
        json_line_bs = process_me_queue.get()

        json_line_str = json_line_bs.decode("utf-8")
        json_line = json.loads(json_line_str)

        title = json_line.get("title", None)
        if title is None:
            continue

        override_value = handle_override_value(override_dict, title)
        if override_value is not None:
            result_queue.put((title, override_value))
            continue

        source_text = json_line.get("source_text", None)
        if source_text is None:
            continue

        categories = json_line["category"]
        if "German non-lemma forms" in categories:
            # never mind
            continue

        genders = wikitext.get_genders_from_page(title, source_text)
        if genders == GenderFlag(0):
            continue
        result_queue.put((title, genders))


def enqueue_json_lines(bz2_buffer: io.BufferedIOBase, process_me_queue: multiprocessing.Queue) -> None:
    while (json_line_bs := read_to_newline(bz2_buffer)):
        process_me_queue.put(json_line_bs)


def process_results(result_queue: multiprocessing.Queue) -> None:
    # TODO: pump into database
    while True:
        (title, genders) = result_queue.get()
        print(title, repr(genders))


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

    override_dict = get_override_dict(config)

    # set up the multiprocessing fun
    process_me_queue = multiprocessing.Queue()
    result_queue = multiprocessing.Queue()

    process_process = multiprocessing.Process(
        target=process_results,
        args=(result_queue,),
        )
    process_process.start()

    for _ in range(16):
        work_process = multiprocessing.Process(
            target=process_json_lines,
            args=(process_me_queue, result_queue, override_dict),
        )
        work_process.start()

    # I will take over as the work distributor process
    for content_json_bz2_path in config["content_json_bz2_paths"]:
        print(f"{content_json_bz2_path}", file=sys.stderr)
        with open(content_json_bz2_path, "rb") as bz2_file:
            with ByteIoProgressWrapper(bz2_file) as bz2_progress:
                with bz2.BZ2File(bz2_progress, "r") as bz2_decompressor:
                    with io.BufferedReader(bz2_decompressor) as bz2_buffer:
                        enqueue_json_lines(bz2_buffer, process_me_queue)

if __name__ == "__main__":
    main()
