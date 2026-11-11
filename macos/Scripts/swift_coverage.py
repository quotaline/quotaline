# /// script
# requires-python = ">=3.15"
# dependencies = []
# ///

"""Reject uncovered Swift behavior and tests that reach live system boundaries."""

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Final, TypedDict, cast

if TYPE_CHECKING:
    from collections.abc import Sequence


class _Count(TypedDict):
    count: int
    covered: int


class _Summary(TypedDict):
    lines: _Count
    regions: _Count


class _File(TypedDict):
    filename: str
    summary: _Summary


class _Function(TypedDict):
    count: int
    filenames: list[str]
    name: str
    regions: list[list[int]]


class _Unit(TypedDict):
    files: list[_File]
    functions: list[_Function]


class _Coverage(TypedDict):
    data: list[_Unit]


def main(arguments: Sequence[str]) -> None:
    """Use all test bundles and profiles so XCTest and Swift Testing both count."""
    (directory,) = arguments
    binaries: Final = sorted(Path(directory).glob("*.xctest"))
    profiles: Final = sorted((Path(directory) / "codecov").glob("*.profraw"))
    if not profiles:
        sys.exit(f"no coverage profiles under {directory}/codecov")
    if not binaries:
        sys.exit(f"no test bundles under {directory}")
    profile: Final = Path(directory) / "codecov/merged.profdata"
    _tool("llvm-profdata", "merge", "-sparse", *(str(path) for path in profiles), "-o", str(profile))
    objects: Final = [str(binaries[0] / "Contents/MacOS" / binaries[0].stem)]
    for bundle in binaries[1:]:
        objects.extend(("-object", str(bundle / "Contents/MacOS" / bundle.stem)))
    data: Final = cast("_Coverage", json.loads(_tool("llvm-cov", "export", *objects, "-instr-profile", str(profile))))
    _check_isolation(data)
    ignored: Final = _glue_exclusions()
    sys.stdout.write(_tool("llvm-cov", "report", *objects, "-instr-profile", str(profile), ignored, "-use-color=false"))
    report: Final = cast(
        "_Coverage",
        json.loads(_tool("llvm-cov", "export", *objects, "-instr-profile", str(profile), ignored)),
    )
    missing: Final = [
        entry
        for unit in report["data"]
        for entry in unit["files"]
        if any(entry["summary"][kind]["covered"] < entry["summary"][kind]["count"] for kind in ("lines", "regions"))
    ]
    if missing:
        Path("../target").mkdir(exist_ok=True)
        Path("../target/macos-swift-coverage.json").write_text(json.dumps(report), encoding="utf-8")
        for entry in missing:
            counts: Final = entry["summary"]
            sys.stdout.write(
                f"{entry['filename']}: {counts['lines']['count'] - counts['lines']['covered']} lines, "
                f"{counts['regions']['count'] - counts['regions']['covered']} regions\n"
            )
            listing: Final = _tool(
                "llvm-cov", "show", *objects, "-instr-profile", str(profile), "-use-color=false", entry["filename"]
            )
            sys.stdout.write(
                "\n".join(line for line in listing.splitlines() if re.match(r"\s*\d+\|\s*0\|", line)) + "\n"
            )
        sys.exit("Swift coverage requires every measured line and region")
    sys.stdout.write("coverage gate passed: every line and region in Core, UI, and Widgets executed\n")


def _tool(*arguments: str) -> str:
    return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] - LLVM subcommands and paths are passed as argv, never shell code.
        ["/usr/bin/xcrun", *arguments], check=True, capture_output=True, text=True, timeout=120
    ).stdout


def _check_isolation(report: _Coverage) -> None:
    boundary: Final = re.compile(
        r"/(SystemKeychain|SystemHTTPTransport|PersistentDefaults|ApplicationActivation|SystemLoginLauncher)\.swift$"
    )
    live: Final = [
        function
        for unit in report["data"]
        for function in unit["functions"]
        if function["count"] > 0
        and (
            boundary.search(function["filenames"][0])
            or (
                function["filenames"][0].endswith("/LibprocProcessScanner.swift")
                and re.search(r"processes|listeningPorts|procargs", function["name"])
            )
        )
    ]
    if live:
        sys.exit(f"test isolation failed: {len(live)} live system-boundary functions ran")
    sys.stdout.write(
        "test isolation passed: no live credential, HTTP, preference, activation, or process-boundary calls\n"
    )


def _glue_exclusions() -> str:
    # These adapters require a host or version-bound framework; cap them so unmeasured logic cannot accumulate.
    glue: Final = (
        "src/Quotaline/main.swift",
        "src/QuotalineUI/WorkspaceGlue.swift",
        "src/QuotalineUI/Adapters/LaunchAtLoginService.swift",
        "src/QuotalineUI/Adapters/ApplicationActivation.swift",
        "src/QuotalineUI/Adapters/SystemLoginLauncher.swift",
        "src/QuotalineUI/Adapters/PanelMaterialAdapter.swift",
        "src/QuotalineUI/Adapters/NotificationCenterGlue.swift",
        "src/QuotalineCore/PersistentDefaults.swift",
        "src/QuotalineCore/Credentials/SystemKeychain.swift",
        "src/QuotalineCore/HTTP/SystemHTTPTransport.swift",
        "src/QuotalineCore/Logging/SystemSignposter.swift",
        "src/QuotalineCore/Providers/Antigravity/LibprocProcessScanner.swift",
        "src/QuotalineWidgets/WidgetKitGlue.swift",
        "src/QuotalineApp/SparkleUpdater.swift",
        "src/QuotalineWidgetApp/WidgetBundle.swift",
    )
    budget: Final = 40
    for name in glue:
        lines: Final = len(re.findall(r"^\s*[a-zA-Z@#}]", Path(name).read_text(encoding="utf-8"), flags=re.MULTILINE))
        if lines > budget:
            sys.exit(f"{name} has {lines} lines of code; unmeasured glue must stay under {budget}")
        sys.stdout.write(f"unmeasured glue: {name} ({lines} lines, capped at {budget})\n")
    return "-ignore-filename-regex=(\\.build|tests|src/QuotalineRust/|" + "|".join(map(re.escape, glue)) + ")"


__all__ = ["main"]

if __name__ == "__main__":
    main(sys.argv[1:])
