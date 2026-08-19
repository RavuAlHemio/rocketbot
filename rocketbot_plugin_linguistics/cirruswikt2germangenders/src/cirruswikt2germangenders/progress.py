from collections.abc import Buffer
from os import SEEK_END, SEEK_SET
import sys
from typing import BinaryIO, Iterable, override

class ByteIoProgressWrapper(BinaryIO):
    def __init__(self, inner: BinaryIO) -> None:
        self._inner: BinaryIO = inner
        self._pos: int = 0
        self._last_pos: int = 0
        self._size: int|None = None

        if self._inner.seekable():
            pos = self._inner.tell()
            self._inner.seek(0, SEEK_END)
            self._size = self._inner.tell()
            self._inner.seek(pos, SEEK_SET)

    def _advance_pos(self, advance: int) -> None:
        self._pos += advance
        if self._pos - self._last_pos > 1024*1024:
            if self._size is not None:
                percentage = self._pos * 100 / self._size
                print(f"{self._pos:,} B / {self._size:,} B ({percentage:.02}%)", file=sys.stderr)
            else:
                print(f"{self._pos:,} B", file=sys.stderr)
            self._last_pos = self._pos

    @property
    @override
    def mode(self) -> str:
        return self._inner.mode

    @property
    @override
    def name(self) -> str:
        return self._inner.name

    @override
    def close(self) -> None:
        self._inner.close()

    @property
    @override
    def closed(self) -> bool:
        return self._inner.closed

    @override
    def fileno(self) -> int:
        return self._inner.fileno()

    @override
    def flush(self) -> None:
        self._inner.flush()

    @override
    def isatty(self) -> bool:
        return self._inner.isatty()

    @override
    def read(self, n: int = -1) -> bytes:
        bs = self._inner.read(n)
        self._advance_pos(len(bs))
        return bs

    @override
    def readable(self) -> bool:
        return self._inner.readable()

    @override
    def readline(self, limit: int = -1) -> bytes:
        bs = self._inner.readline(limit)
        self._advance_pos(len(bs))
        return bs

    @override
    def readlines(self, hint: int = -1) -> list[bytes]:
        lines = self._inner.readlines(hint)
        for line in lines:
            self._advance_pos(len(line))
        return lines

    @override
    def seek(self, offset: int, whence: int = 0) -> int:
        self._pos = self._inner.seek(offset, whence)
        self._advance_pos(0)
        return self._pos

    @override
    def seekable(self) -> bool:
        return self._inner.seekable()

    @override
    def tell(self) -> int:
        return self._inner.tell()

    @override
    def truncate(self, size: int|None = None) -> int:
        return self._inner.truncate(size)

    @override
    def writable(self) -> bool:
        return self._inner.writable()

    @override
    def write(self, s: Buffer) -> int:
        count = self._inner.write(s)
        self._advance_pos(count)
        return count

    @override
    def writelines(self, lines: Iterable[Buffer]) -> None:
        self._inner.writelines(lines)
        for buf in lines:
            with memoryview(buf) as buf_view:
                self._advance_pos(len(buf_view))

    @override
    def __enter__(self) -> "ByteIoProgressWrapper":
        return self

    @override
    def __exit__(self, type, value, traceback) -> None:
        _ = type
        _ = value
        _ = traceback
        self._inner.close()
