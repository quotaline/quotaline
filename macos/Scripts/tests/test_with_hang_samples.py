"""Use disposable subprocesses to check silence deadlines and cleanup."""

import contextlib
import os
import runpy
import selectors
import signal
import subprocess
import sys
from io import BufferedReader
from itertools import count
from pathlib import Path
from typing import TYPE_CHECKING, Final

import psutil
import pytest

from Scripts.with_hang_samples import main

if TYPE_CHECKING:
    from unittest.mock import MagicMock

    from pytest_mock import MockerFixture

_TIMEOUT: Final = 124
_FAILURE: Final = 7


@pytest.mark.parametrize("status", [0, 7], ids=["success", "failure"])
def test_command_output_and_exit_status(tmp_path: Path, status: int, capfd: pytest.CaptureFixture[str]) -> None:
    """Output must reach both the terminal and the retained artifact."""
    result: Final = main(["1", str(tmp_path), sys.executable, "-c", f"print('test output');raise SystemExit({status})"])
    assert (result, (tmp_path / "output.log").read_text(), capfd.readouterr().out) == (
        status,
        "test output\n",
        "test output\n",
    )


@pytest.mark.parametrize("arguments", [["0", "unused", "true"], ["1", "unused"]], ids=["invalid-timeout", "no-command"])
def test_invalid_invocation_fails(arguments: list[str]) -> None:
    """Missing commands and nonpositive deadlines are rejected before spawning."""
    with pytest.raises(SystemExit, match="positive idle minutes"):
        main(arguments)


def test_signal_exit_uses_shell_status(tmp_path: Path) -> None:
    """CI must see the same status as it did through the shell wrapper."""
    expected: Final = 143
    assert (
        main(["1", str(tmp_path), sys.executable, "-c", "import os,signal;os.kill(os.getpid(),signal.SIGTERM)"])
        == expected
    )


def test_silence_stops_command(tmp_path: Path, capfd: pytest.CaptureFixture[str]) -> None:
    """A silent process cannot hold a runner after the deadline."""
    result: Final = main(
        ["0.002", str(tmp_path), sys.executable, "-c", "import os,time;print(os.getpid(),flush=True);time.sleep(30)"]
    )
    assert result == _TIMEOUT
    assert not psutil.pid_exists(int((tmp_path / "output.log").read_text()))
    assert "No test output" in capfd.readouterr().err


def test_output_renews_deadline(tmp_path: Path, mocker: MockerFixture) -> None:
    """A controlled clock keeps runner load from turning progress into a timeout."""
    process: Final = mocker.create_autospec(
        subprocess.Popen, instance=True, pid=123, stdout=mocker.create_autospec(BufferedReader, instance=True)
    )
    process.__enter__.return_value = process
    process.poll.return_value = None
    process.wait.return_value = 0
    mocker.patch("subprocess.Popen", autospec=True, return_value=process)
    selector: Final = mocker.create_autospec(selectors.DefaultSelector, instance=True)
    selector.__enter__.return_value = selector
    readable: Final = [(selectors.SelectorKey(1, 1, selectors.EVENT_READ, None), selectors.EVENT_READ)]
    selector.select.side_effect = [readable, []] * 4 + [readable]
    mocker.patch("selectors.DefaultSelector", autospec=True, return_value=selector)
    mocker.patch(
        "time.monotonic",  # ruff: ignore[shared-clock-mock] - this test runs without concurrent work
        autospec=True,
        side_effect=count(step=10),
    )
    mocker.patch("os.read", autospec=True, side_effect=[f"{index}\n".encode() for index in range(4)] + [b""])
    mocker.patch("os.killpg", autospec=True)
    mocker.patch("psutil.Process", autospec=True, side_effect=psutil.NoSuchProcess(process.pid))

    result: Final = main(["0.5", str(tmp_path), "test-command"])

    assert (result, (tmp_path / "output.log").read_text()) == (0, "0\n1\n2\n3\n")


def test_finished_command_without_read_event(tmp_path: Path, mocker: MockerFixture) -> None:
    """A process exit must be noticed even if the selector returns no events."""
    mocker.patch.object(selectors.DefaultSelector, "select", autospec=True, return_value=[])
    assert main(["1", str(tmp_path), sys.executable, "-c", "raise SystemExit(7)"]) == _FAILURE


@pytest.fixture
def sampled_process(mocker: MockerFixture) -> MagicMock:
    """Limit stack sampling to the process tree represented by the subprocess boundary."""
    process: Final = mocker.create_autospec(psutil.Process, instance=True, pid=123)
    process.name.return_value = "PackageTests"
    process.children.return_value = []
    mocker.patch("psutil.Process", autospec=True, return_value=process)
    return process


def test_hang_samples_test_process(tmp_path: Path, sampled_process: MagicMock, mocker: MockerFixture) -> None:
    """The stack sample must identify the test process and stay in its artifact directory."""
    sample: Final = mocker.patch("subprocess.run", autospec=True)
    assert main(["0.001", str(tmp_path), sys.executable, "-c", "import time;time.sleep(30)"]) == _TIMEOUT
    assert sample.call_args.args[0] == [
        "/usr/bin/sample",
        str(sampled_process.pid),
        "5",
        "-file",
        str(tmp_path / "sample-123.txt"),
    ]


def test_sampling_process_exit_is_tolerated(tmp_path: Path, mocker: MockerFixture) -> None:
    """A process that exits during enumeration must not prevent timeout cleanup."""
    mocker.patch("psutil.Process", autospec=True, side_effect=psutil.NoSuchProcess(123))
    assert main(["0.001", str(tmp_path), sys.executable, "-c", "import time;time.sleep(30)"]) == _TIMEOUT


def test_unresponsive_command_is_killed(tmp_path: Path, mocker: MockerFixture) -> None:
    """Escalate termination when a child ignores SIGTERM."""
    wait: Final = subprocess.Popen.wait

    def bounded_wait(process: subprocess.Popen[bytes], timeout: float | None = None) -> int:
        return wait(process, timeout=0.01 if timeout is not None else None)

    mocker.patch.object(subprocess.Popen, "wait", autospec=True, side_effect=bounded_wait)
    result: Final = main(
        [
            "0.002",
            str(tmp_path),
            sys.executable,
            "-c",
            (
                "import os,signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);"
                "print(os.getpid(),flush=True);time.sleep(30)"
            ),
        ]
    )
    assert result == _TIMEOUT
    assert not psutil.pid_exists(int((tmp_path / "output.log").read_text()))


def test_cli_returns_status(tmp_path: Path, mocker: MockerFixture) -> None:
    """The command-line wrapper preserves failure status for CI."""
    mocker.patch.object(
        sys, "argv", ["with_hang_samples.py", "1", str(tmp_path), sys.executable, "-c", "raise SystemExit(7)"]
    )
    with pytest.raises(SystemExit) as result:
        runpy.run_path(str(Path(__file__).resolve().parents[1] / "with_hang_samples.py"), run_name="__main__")
    assert result.value.code == _FAILURE


def test_timeout_kills_a_descendant_after_its_parent_exits(tmp_path: Path) -> None:
    """A responsive parent must not hide a descendant that ignores termination."""
    child: Final = (
        "import os,signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);"
        "print(os.getpid(),flush=True);time.sleep(30)"
    )
    try:
        result: Final = main(
            [
                "0.01",
                str(tmp_path),
                sys.executable,
                "-c",
                (
                    f"import os,subprocess,sys,time;print(os.getpid(),flush=True);"
                    f"subprocess.Popen([sys.executable,'-c',{child!r}]);time.sleep(30)"
                ),
            ]
        )
        child_pid: Final = int((tmp_path / "output.log").read_text().splitlines()[1])
        assert result == _TIMEOUT
        psutil.wait_procs([process for process in psutil.process_iter() if process.pid == child_pid], timeout=1)
        assert child_pid not in {
            process.pid for process in psutil.process_iter(["status"]) if process.info["status"] != psutil.STATUS_ZOMBIE
        }
    finally:
        parent_pid: Final = int((tmp_path / "output.log").read_text().splitlines()[0])
        with contextlib.suppress(ProcessLookupError):
            os.killpg(parent_pid, signal.SIGKILL)
