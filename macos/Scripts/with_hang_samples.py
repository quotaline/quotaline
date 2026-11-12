# /// script
# requires-python = ">=3.15"
# dependencies = ["psutil==7.2.2"]
# ///

"""Preserve stack samples when a test command stops producing output."""

import contextlib
import os
import selectors
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import TYPE_CHECKING, Final, cast

import psutil

if TYPE_CHECKING:
    from collections.abc import Sequence
    from typing import BinaryIO


def main(arguments: Sequence[str]) -> int:
    """Bound silence while keeping the command's output and exit status."""
    minutes, destination, *command = arguments
    idle_seconds: Final = float(minutes) * 60
    if idle_seconds <= 0 or not command:
        sys.exit("Pass positive idle minutes, a sample directory, and a test command")
    directory: Final = Path(destination)
    directory.mkdir(parents=True, exist_ok=True)
    with (
        (directory / "output.log").open("wb") as output,
        subprocess.Popen(  # ruff: ignore[subprocess-without-shell-equals-true] - the recipe supplies argv; no shell interpolation occurs.
            command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True
        ) as process,
        selectors.DefaultSelector() as selector,
    ):
        try:
            pipe: Final = cast("BinaryIO", process.stdout)
            selector.register(pipe, selectors.EVENT_READ)
            updated = time.monotonic()
            while True:
                if selector.select(timeout=min(1, max(0, idle_seconds - (time.monotonic() - updated)))):
                    if not (data := os.read(pipe.fileno(), 65536)):
                        break
                    output.write(data)
                    output.flush()
                    sys.stdout.buffer.write(data)
                    sys.stdout.buffer.flush()
                    updated = time.monotonic()
                elif process.poll() is not None:
                    break
                elif time.monotonic() - updated >= idle_seconds:
                    sys.stderr.write(f"::error::No test output for {minutes} minutes; sampling into {directory}\n")
                    _sample(process.pid, directory)
                    return 124
            status: Final = process.wait()
            return 128 - status if status < 0 else status
        finally:
            _stop(process)


def _sample(pid: int, directory: Path) -> None:
    try:
        parent: Final = psutil.Process(pid)
        for process in (parent, *parent.children(recursive=True)):
            if any(name in process.name() for name in ("xctest", "swiftpm-testing-helper", "PackageTests")):
                subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] - sample receives only a PID from this command's own process tree.
                    ["/usr/bin/sample", str(process.pid), "5", "-file", str(directory / f"sample-{process.pid}.txt")],
                    check=False,
                    capture_output=True,
                    timeout=15,
                )
    except psutil.NoSuchProcess:
        sys.stderr.write("Test process exited before sampling\n")


def _stop(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    with contextlib.suppress(subprocess.TimeoutExpired):
        process.wait(timeout=5)
    with contextlib.suppress(ProcessLookupError):
        os.killpg(process.pid, signal.SIGKILL)
    process.wait()


__all__ = ["main"]

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
