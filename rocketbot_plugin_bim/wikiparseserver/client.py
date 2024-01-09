#!/usr/bin/env python3
import argparse
import io
import socket
import sys
from typing import NamedTuple


START_MAGIC = b"WiKiCrUnCh"
END_MAGIC = b"EnOuGhWiKi"


class Utf8Encoded(NamedTuple):
    data: bytes
    length_bytes: bytes


def encode_utf8(text: str, description: str) -> Utf8Encoded:
    text_utf8 = text.encode("utf-8")
    if len(text_utf8) > 0xFFFFFFFF:
        raise ValueError(f"{description} is too long ({len(text_utf8)} UTF-8 bytes); maximum is {0xFFFFFFFF})")
    length_bytes = len(text_utf8).to_bytes(4, byteorder="big")
    return Utf8Encoded(text_utf8, length_bytes)


def send_strict(sock: socket.socket, data: bytes):
    total_sent = 0
    while total_sent < len(data):
        this_sent = sock.send(data[total_sent:])
        if this_sent == 0:
            raise SystemError("zero bytes sent on socket")
        total_sent += this_sent


def recv_strict(sock: socket.socket, count: int) -> bytearray:
    data = bytearray(count)
    data_view = memoryview(data)
    position = 0
    while position < count:
        bytes_read = sock.recv_into(data_view[position:])
        if bytes_read == 0:
            raise SystemError("zero bytes received on socket")
        position += bytes_read
    return data


def parse_wikitext(
    host: str,
    port: int,
    title: str,
    input_file: io.TextIOBase,
    output_file: io.TextIOBase,
) -> None:
    # connect to socket
    sock = socket.create_connection((host, port))

    # read in the wikitext
    with input_file:
        wikitext = input_file.read()

    # encode everything as UTF-8
    (title_utf8, title_length_bytes) = encode_utf8(title, "title")
    (wikitext_utf8, wikitext_length_bytes) = encode_utf8(wikitext, "wikitext")

    send_strict(sock, START_MAGIC)
    send_strict(sock, title_length_bytes)
    send_strict(sock, title_utf8)
    send_strict(sock, wikitext_length_bytes)
    send_strict(sock, wikitext_utf8)

    # read out the length
    xhtml_length_bytes = recv_strict(sock, 4)
    xhtml_length = int.from_bytes(xhtml_length_bytes, byteorder="big")

    # read the XHTML
    xhtml_bytes = recv_strict(sock, xhtml_length)

    # tell server to stop
    send_strict(sock, END_MAGIC)

    # decode XHTML as UTF-8
    xhtml = xhtml_bytes.decode("utf-8")

    # copy it out to the output file
    with output_file:
        output_file.write(xhtml)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-p", "--port",
        dest="port", type=int, required=True,
        help="The port on which to contact wikiparseserver.php.",
    )
    parser.add_argument(
        "-t", "--target",
        dest="target", type=str, default="localhost",
        help="The host on which to contact wikiparseserver.php. The default is localhost.",
    )
    parser.add_argument(
        dest="title",
        help="The title of the article to parse.",
    )
    parser.add_argument(
        dest="input_file", type=argparse.FileType("r"), nargs="?", default=sys.stdin,
        help="The file containing the wikitext to parse.",
    )
    parser.add_argument(
        dest="output_file", type=argparse.FileType("w"), nargs="?", default=sys.stdout,
        help="The file into which to write the XHTML output.",
    )
    args = parser.parse_args()

    parse_wikitext(
        args.target,
        args.port,
        args.title,
        args.input_file,
        args.output_file,
    )


if __name__ == "__main__":
    main()
