"""Keep the coverage gate strict without starting Swift tests or touching a desktop."""

import json
import runpy
import subprocess
import sys
from pathlib import Path
from shutil import copytree
from typing import TYPE_CHECKING, Final

import pytest

from Scripts.swift_coverage import main

if TYPE_CHECKING:
    from unittest.mock import MagicMock

    from pytest_mock import MockerFixture


@pytest.fixture
def workspace(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """Use disposable profiles while checking the real adapter-size limits."""
    source: Final = Path(__file__).resolve().parents[2]
    copytree(source / "src", tmp_path / "macos/src")
    monkeypatch.chdir(tmp_path / "macos")
    directory: Final = tmp_path / "bin"
    (directory / "codecov").mkdir(parents=True)
    for name in ("swift-testing", "xctest"):
        (directory / "codecov" / f"{name}.profraw").touch()
        (directory / f"{name}.xctest").mkdir()
    return directory


@pytest.fixture
def commands(mocker: MockerFixture) -> MagicMock:
    """LLVM is the process boundary; files and coverage decisions stay real."""
    return mocker.patch("subprocess.run", autospec=True)


@pytest.fixture
def report() -> str:
    """Provide one measured source and one inactive live-system adapter."""
    return json.dumps(
        {
            "data": [
                {
                    "files": [
                        {
                            "filename": "src/QuotalineCore/Model.swift",
                            "summary": {"lines": {"count": 2, "covered": 2}, "regions": {"count": 3, "covered": 3}},
                        }
                    ],
                    "functions": [
                        {
                            "count": 0,
                            "filenames": ["src/QuotalineCore/SystemKeychain.swift"],
                            "name": "read",
                            "regions": [],
                        }
                    ],
                }
            ]
        }
    )


def test_complete_coverage_merges_both_frameworks(
    workspace: Path, commands: MagicMock, report: str, capsys: pytest.CaptureFixture[str]
) -> None:
    """A passing report must include both test binaries and both raw profiles."""
    commands.side_effect = [
        subprocess.CompletedProcess([], 0, stdout=value) for value in ("", report, "report\n", report)
    ]
    main([str(workspace)])
    assert "coverage gate passed" in capsys.readouterr().out
    assert commands.call_args_list[0].args[0] == [
        "/usr/bin/xcrun",
        "llvm-profdata",
        "merge",
        "-sparse",
        str(workspace / "codecov/swift-testing.profraw"),
        str(workspace / "codecov/xctest.profraw"),
        "-o",
        str(workspace / "codecov/merged.profdata"),
    ]
    assert commands.call_args_list[1].args[0][3:6] == [
        str(workspace / "swift-testing.xctest/Contents/MacOS/swift-testing"),
        "-object",
        str(workspace / "xctest.xctest/Contents/MacOS/xctest"),
    ]


@pytest.mark.parametrize("missing", ["profiles", "bundles"])
def test_missing_instrumentation_fails(workspace: Path, missing: str) -> None:
    """An empty build cannot claim full coverage."""
    for path in workspace.glob("codecov/*.profraw" if missing == "profiles" else "*.xctest"):
        if path.is_dir():
            path.rmdir()
        else:
            path.unlink()
    with pytest.raises(SystemExit, match=r"no coverage profiles|no test bundles"):
        main([str(workspace)])


@pytest.mark.parametrize(
    "case",
    [
        ("SystemKeychain", "read", True, True),
        ("LibprocProcessScanner", "processes", True, True),
        ("LibprocProcessScanner", "staticHelper", True, False),
        ("Model", "read", True, False),
        ("SystemHTTPTransport", "request", False, False),
    ],
    ids=["keychain", "processes", "scanner-helper", "ordinary-model", "inactive-http"],
)
def test_live_boundary_detection(
    workspace: Path,
    commands: MagicMock,
    report: str,
    case: tuple[str, str, bool, bool],
    capsys: pytest.CaptureFixture[str],
) -> None:
    """Only executed live-I/O functions break test isolation."""
    name, symbol, active, fails = case
    raw: Final = json.dumps(
        {"data": [{"functions": [{"count": int(active), "filenames": [f"/src/{name}.swift"], "name": symbol}]}]}
    )
    commands.side_effect = [subprocess.CompletedProcess([], 0, stdout=value) for value in ("", raw, "report\n", report)]
    if fails:
        with pytest.raises(SystemExit, match="test isolation failed"):
            main([str(workspace)])
    else:
        main([str(workspace)])
        assert "coverage gate passed" in capsys.readouterr().out


@pytest.mark.parametrize("kind", ["lines", "regions"])
def test_uncovered_behavior_writes_diagnostics(
    workspace: Path, commands: MagicMock, report: str, kind: str, capsys: pytest.CaptureFixture[str]
) -> None:
    """Line and region gaps each fail and preserve the exported report."""
    incomplete: Final = report.replace(
        '"lines": {"count": 2, "covered": 2}' if kind == "lines" else '"regions": {"count": 3, "covered": 3}',
        '"lines": {"count": 2, "covered": 1}' if kind == "lines" else '"regions": {"count": 3, "covered": 2}',
    )
    commands.side_effect = [
        subprocess.CompletedProcess([], 0, stdout=value)
        for value in ("", report, "report\n", incomplete, " 1| 1|covered\n 2| 0|missed\n")
    ]
    with pytest.raises(SystemExit, match="every measured line and region"):
        main([str(workspace)])
    assert json.loads(Path("../target/macos-swift-coverage.json").read_bytes()) == json.loads(incomplete)
    assert "2| 0|missed" in capsys.readouterr().out


def test_glue_cannot_hide_logic(workspace: Path, commands: MagicMock, report: str) -> None:
    """Excluded adapters remain capped even when the measured report is complete."""
    Path("src/Quotaline/main.swift").write_text("statement\n" * 41, encoding="utf-8")
    commands.side_effect = [subprocess.CompletedProcess([], 0, stdout=value) for value in ("", report)]
    with pytest.raises(SystemExit, match="must stay under 40"):
        main([str(workspace)])


def test_llvm_failure_propagates(workspace: Path, commands: MagicMock) -> None:
    """A failed profile merge cannot be mistaken for a coverage result."""
    commands.side_effect = subprocess.CalledProcessError(1, ["xcrun", "llvm-profdata"])
    with pytest.raises(subprocess.CalledProcessError):
        main([str(workspace)])


def test_cli_uses_supplied_directory(
    workspace: Path, commands: MagicMock, report: str, mocker: MockerFixture, capsys: pytest.CaptureFixture[str]
) -> None:
    """The script entry point applies the same gate as the recipe."""
    commands.side_effect = [
        subprocess.CompletedProcess([], 0, stdout=value) for value in ("", report, "report\n", report)
    ]
    mocker.patch.object(sys, "argv", ["swift_coverage.py", str(workspace)])
    runpy.run_path(str(Path(__file__).resolve().parents[1] / "swift_coverage.py"), run_name="__main__")
    assert "coverage gate passed" in capsys.readouterr().out
