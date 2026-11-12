# /// script
# requires-python = ">=3.15"
# dependencies = ["psutil==7.2.2"]
# ///

"""Keep local Xcode UI verification separate from installed apps and protected volumes."""

import argparse
import fcntl
import json
import os
import platform
import plistlib
import signal
import subprocess
import sys
import tempfile
import time
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from shutil import copytree, disk_usage, which
from typing import TYPE_CHECKING, Final

import psutil

if TYPE_CHECKING:
    from types import FrameType
    from typing import TextIO

_REPOSITORY: Final = Path(__file__).resolve().parent.parent


@dataclass(frozen=True)
class UISuite:
    """Keep test products and sandbox identities separate between suites."""

    target: str
    runner: str
    scheme: str
    derived: str


_SUITES: Final = {
    "functional": UISuite(
        "QuotalineApplicationUITests",
        "tech.bernat.quotaline.application-ui-tests.xctrunner",
        "Quotaline-Direct",
        ".build/ui-derived",
    ),
    "performance": UISuite(
        "QuotalinePerformanceTests",
        "tech.bernat.quotaline.performance-tests.xctrunner",
        "Quotaline-Benchmarks",
        ".build/benchmark-derived",
    ),
}
_GIB: Final = 1024**3
type JSONValue = dict[str, JSONValue] | list[JSONValue] | str | int | float | bool | None


def main(arguments: list[str] | None = None) -> int:
    """Reject unknown selectors before building; prepare-only never starts the runner."""
    parser: Final = argparse.ArgumentParser(description="Run selected mock-only Xcode UI tests on this desktop.")
    parser.add_argument("--prepare-only", action="store_true", help="Build and validate without starting UI tests")
    parser.add_argument(
        "--check-results", type=Path, help="Validate an existing result JSON without building or launching"
    )
    parser.add_argument("--suite", choices=list(_SUITES), default="functional")
    parser.add_argument("tests", nargs="+", help="Target/Class/testMethod, or 'all' for the complete inventory")
    options: Final = parser.parse_args(arguments)
    temporary_root: Final = _REPOSITORY.parent / "target/tmp"
    temporary_root.mkdir(parents=True, exist_ok=True)
    os.environ["TMPDIR"] = str(temporary_root)
    suite: Final = _SUITES[options.suite]
    tests: Final = select_tests(_REPOSITORY / "tests/ui-groups.json", options.tests, suite=options.suite)
    if options.check_results is not None:
        validate_results(options.check_results, tests)
        sys.stdout.write(f"Verified {len(tests)} selected UI tests passed once, without skips or retries.\n")
        return 0
    if any("/testLong" in test for test in tests):
        _executable(["tesseract"])
    if not options.prepare_only:
        if options.suite == "performance":
            _run(["Scripts/check-benchmark-runtime.sh"], timeout=30)
        check_desktop()
        _check_automation()
    os.chdir(_REPOSITORY)
    descriptor: Final = os.open(
        temporary_root / f"quotaline-ui-{os.getuid()}.lock", os.O_CREAT | os.O_WRONLY | os.O_NOFOLLOW, 0o600
    )
    with os.fdopen(descriptor, "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        with tempfile.TemporaryDirectory(prefix="quotaline-local-ui.", dir=temporary_root) as temporary:
            runtime: Final = Path(temporary)
            for command in (["Scripts/check-test-isolation.sh"], ["Scripts/check-ui-test-inventory.sh"]):
                _run(command, timeout=30)
            _run(["xcodegen", "generate", "--spec", "App/project.yml"], timeout=60)
            build: Final = [
                "xcodebuild",
                "-project",
                "App/Quotaline.xcodeproj",
                "-scheme",
                suite.scheme,
                "-configuration",
                "Verification",
                "-destination",
                f"platform=macOS,arch={os.uname().machine}",
                "-derivedDataPath",
                suite.derived,
                "-clonedSourcePackagesDirPath",
                ".build/xcode-packages",
                "-jobs",
                "1",
                "ONLY_ACTIVE_ARCH=YES",
                "SWIFT_USE_PARALLEL_WHOLE_MODULE_OPTIMIZATION=NO",
                "OTHER_SWIFT_FLAGS=$(inherited) -num-threads 1",
                "build-for-testing",
            ]
            if status := run_bounded(build, runtime, timeout=900):
                return status
            if disk_usage(runtime).free < 2 * _GIB:
                msg: Final = "Need 2 GiB free on the internal disk for UI products and results"
                raise RuntimeError(msg)
            _run(["ditto", f"{suite.derived}/Build/Products", str(runtime / "Products")], timeout=120)
            _seal_runner(runtime / "Products", suite)
            plan: Final = validate_products(runtime / "Products", suite=suite)
            if options.prepare_only:
                sys.stdout.write(
                    f"Validated app, runner, dependencies and {len(tests)} selected methods; no UI launched.\n"
                )
                return 0
            return _execute_tests(runtime, plan, tests, suite)


def _check_automation() -> None:
    status: Final = subprocess.check_output(
        ["/usr/bin/automationmodetool"], text=True, timeout=5, env=os.environ | {"LC_ALL": "C"}
    )
    if "This device DOES NOT REQUIRE user authentication to enable Automation Mode." not in status.splitlines():
        msg: Final = (
            "UI tests require unattended Automation Mode before launch. An administrator must configure it once with "
            "sudo /usr/bin/automationmodetool enable-automationmode-without-authentication. "
            "No test runner started and no security setting changed."
        )
        raise RuntimeError(msg)


def select_tests(inventory: Path, requested: list[str], *, suite: str = "functional") -> list[str]:
    """Require an exact subset of the CI inventory, preserving requested order."""
    groups: Final = json.loads(inventory.read_text(encoding="utf-8"))
    declared: Final = [
        f"{_SUITES[suite].target}/{test}"
        for group, tests in groups.items()
        if (group == "performance") == (suite == "performance")
        for test in tests
    ]
    selected: Final = declared if requested == ["all"] else requested
    if not selected or len(set(selected)) != len(selected) or not set(selected) <= set(declared):
        msg: Final = "Select existing methods exactly once, or 'all'; unknown and duplicate selectors are errors"
        raise ValueError(msg)
    return selected


def _seal_runner(products: Path, suite: UISuite) -> None:
    """Seal the outer runner after Xcode updates its test plug-in and debug symbols."""
    _, _, runner = _validate_product_layout(products, suite)
    _run(
        [
            "/usr/bin/codesign",
            "--verify",
            "--deep",
            "--strict",
            str(runner / "Contents/PlugIns" / f"{suite.target}.xctest"),
        ],
        timeout=30,
    )
    _run(
        [
            "/usr/bin/codesign",
            "--force",
            "--sign",
            "-",
            "--preserve-metadata=identifier,entitlements,flags,runtime",
            str(runner),
        ],
        timeout=30,
    )


def validate_products(products: Path, *, suite: UISuite = _SUITES["functional"]) -> Path:
    """Reject distribution builds, external dependencies and ambiguous test plans before launch."""
    plan, app, runner = _validate_product_layout(products, suite)
    for bundle in (app, runner):
        _run(["codesign", "--verify", "--deep", "--strict", str(bundle)], timeout=30)
    return plan


def _validate_product_layout(products: Path, suite: UISuite) -> tuple[Path, Path, Path]:
    products = products.resolve(strict=True)
    plans: Final = list(products.glob("*.xctestrun"))
    if len(plans) != 1:
        msg = "Expected exactly one xctestrun file"
        raise ValueError(msg)
    plan: Final[JSONValue] = plistlib.loads(plans[0].read_bytes())
    for value in _strings(plan):
        if "/Volumes/" in value or any(f"/{name}/" in value for name in ("Desktop", "Documents", "Downloads")):
            msg = f"Privacy-protected test plan path: {value}"
            raise ValueError(msg)
    for path in products.rglob("*"):
        if path.is_symlink() and not path.resolve(strict=True).is_relative_to(products):
            msg = f"Test products have an external symlink: {path}"
            raise ValueError(msg)
    target: Final = _test_target(plan, suite)
    if (
        target.get("PreferredScreenCaptureFormat") != "screenshots"
        or target.get("SystemAttachmentLifetime") != "keepNever"
    ):
        msg = "Test plan must disable automatic capture; scenarios retain their own screenshots"
        raise ValueError(msg)
    _validate_target_paths(target, products, suite)
    app: Final = products / "Verification/QuotalineDirect.app"
    info: Final = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    if info.get("CFBundleIdentifier") != "tech.bernat.quotaline.verification" or info.get(
        "TMBVerificationOnly"
    ) not in {
        True,
        "YES",
    }:
        msg = "Application must use the verification-only build, not Debug or a distribution build"
        raise ValueError(msg)
    runner: Final = products / f"Verification/{suite.target}-Runner.app"
    if plistlib.loads((runner / "Contents/Info.plist").read_bytes()).get("CFBundleIdentifier") != suite.runner:
        msg = "Runner bundle identity does not match the verification test plan"
        raise ValueError(msg)
    return plans[0], app, runner


def _test_target(plan: JSONValue, suite: UISuite) -> dict[str, JSONValue]:
    if not isinstance(plan, dict) or not isinstance(configurations := plan.get("TestConfigurations"), list):
        msg = "Expected test plan configurations"
        raise TypeError(msg)
    if len(configurations) != 1 or not isinstance(configuration := configurations[0], dict):
        msg = "Expected one test configuration"
        raise ValueError(msg)
    targets: Final = configuration.get("TestTargets")
    if not isinstance(targets, list) or len(targets) != 1 or not isinstance(target := targets[0], dict):
        msg = "Expected one application UI test target"
        raise ValueError(msg)
    if target.get("BlueprintName") != suite.target or target.get("TestHostBundleIdentifier") != suite.runner:
        msg = "Test plan does not target the verification runner"
        raise ValueError(msg)
    return target


def _validate_target_paths(target: dict[str, JSONValue], products: Path, suite: UISuite) -> None:
    expected: Final = {
        "UITargetAppPath": "__TESTROOT__/Verification/QuotalineDirect.app",
        "TestHostPath": f"__TESTROOT__/Verification/{suite.target}-Runner.app",
        "TestBundlePath": f"__TESTHOST__/Contents/PlugIns/{suite.target}.xctest",
    }
    if any(target.get(key) != value for key, value in expected.items()):
        msg = "Test app, runner and bundle must use relocated paths"
        raise ValueError(msg)
    for value in _strings(target):
        for component in value.split(":"):
            if component.startswith("/") and not component.startswith(("/usr/lib/", "/System/Library/")):
                msg = f"Test dependencies must use Xcode placeholders, not absolute paths: {component}"
                raise ValueError(msg)
            if component.startswith("__TESTROOT__/"):
                path = Path(component.replace("__TESTROOT__", str(products))).resolve(strict=True)
                if not path.is_relative_to(products):
                    msg = f"Test dependency escapes Products: {component}"
                    raise ValueError(msg)


def _strings(value: JSONValue) -> list[str]:
    if isinstance(value, dict):
        return [text for item in value.values() for text in _strings(item)]
    if isinstance(value, list):
        return [text for item in value for text in _strings(item)]
    return [value] if isinstance(value, str) else []


def _execute_tests(runtime: Path, plan: Path, tests: list[str], suite: UISuite) -> int:
    artifact_root: Final = Path.home() / "Library/Containers" / suite.runner / "Data/tmp"
    artifact_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="quotaline-ui.", dir=artifact_root) as temporary:
        artifacts: Final = Path(temporary)
        (artifacts / "output").mkdir()
        (artifacts / "profile").mkdir()
        environment: Final = os.environ | {
            "TEST_RUNNER_QUOTALINE_TEST_DESKTOP": "local-verification",
            "TEST_RUNNER_TMB_BENCHMARK_OUTPUT_DIR": str(artifacts / "output"),
            "TEST_RUNNER_TMB_VERIFICATION_ROOT": str(artifacts / "state"),
        }
        if suite == _SUITES["performance"]:
            environment.update(
                TEST_RUNNER_TMB_BENCHMARK_MACOS_MAJOR=platform.mac_ver()[0].split(".", 1)[0],
                TEST_RUNNER_TMB_PROFILE_DIRECTORY=str(artifacts / "profile"),
            )
        command: Final = [
            "xcodebuild",
            "test-without-building",
            "-xctestrun",
            str(plan),
            "-destination",
            f"platform=macOS,arch={os.uname().machine}",
            "-parallel-testing-enabled",
            "NO",
            "-test-timeouts-enabled",
            "YES",
            "-maximum-test-execution-time-allowance",
            "360",
            "-resultBundlePath",
            str(runtime / "tests.xcresult"),
            *[f"-only-testing:{test}" for test in tests],
        ]
        output: Final = Path(tempfile.mkdtemp(prefix="local-ui-results.", dir=_REPOSITORY / ".build"))
        sys.stdout.write(f"Mock tests use the desktop and pointer. Private local results: {output}\n")
        sys.stdout.flush()
        try:
            status: Final = run_bounded(
                command,
                runtime,
                timeout=360 * len(tests) + 60,
                environment=environment,
                cwd=runtime,
            )
            if not (bundle := runtime / "tests.xcresult").exists():
                msg: Final = "Xcode did not produce a result bundle"
                raise RuntimeError(msg)
            with (output / "tests.json").open("w") as stream:
                _run(
                    ["xcrun", "xcresulttool", "get", "test-results", "tests", "--path", str(bundle), "--compact"],
                    stdout=stream,
                    timeout=30,
                )
            validate_results(output / "tests.json", tests)
            _run(
                [
                    str(_REPOSITORY / "Scripts/check-rendered-text.sh"),
                    str(artifacts / "output"),
                    str(sum("/testLong" in test for test in tests)),
                ],
                timeout=300,
            )
            return status
        finally:
            copytree(artifacts, output / "artifacts")
            if (runtime / "tests.xcresult").exists():
                _run(["ditto", str(runtime / "tests.xcresult"), str(output / "tests.xcresult")], timeout=120)


def validate_results(result: Path, tests: list[str]) -> None:
    """Require one passing result per selected method; zero tests and retries fail."""
    records: Final[list[tuple[str, str]]] = []

    def collect(value: JSONValue) -> None:
        if isinstance(value, dict):
            if value.get("nodeType") == "Test Case":
                records.append((str(value.get("nodeIdentifier")), str(value.get("result"))))
            for item in value.values():
                collect(item)
        elif isinstance(value, list):
            for item in value:
                collect(item)

    collect(json.loads(result.read_text(encoding="utf-8")))
    expected: Final = Counter((test.split("/", 1)[1] + "()", "Passed") for test in tests)
    if not expected or Counter(records) != expected:
        msg: Final = f"Tests must pass exactly once, without skips or retries: expected={expected}, actual={records}"
        raise ValueError(msg)


def run_bounded(
    command: list[str],
    runtime: Path,
    *,
    timeout: float,
    environment: dict[str, str] | None = None,
    cwd: Path | None = None,
) -> int:
    """Bound owned RSS to 4 GiB and swap growth to 1 GiB; stop descendants on failure."""
    swap_before: Final = psutil.swap_memory().used
    check_resources(0, swap_before)
    desktop: Final = (environment or {}).get("TEST_RUNNER_QUOTALINE_TEST_DESKTOP") == "local-verification"
    if desktop:
        check_desktop()
        _check_automation()
    started: Final = time.monotonic()
    owned: Final[set[psutil.Process]] = set()
    peak_rss = 0
    sampler: subprocess.Popen[bytes] | None = None
    profile: Final = (environment or {}).get("TEST_RUNNER_TMB_PROFILE_DIRECTORY")
    with subprocess.Popen(  # ruff: ignore[subprocess-without-shell-equals-true] - callers supply argv; selectors are inventory-validated, never shell code.
        _executable(command), env=environment, cwd=cwd, start_new_session=True
    ) as child:
        try:
            try:
                process: Final = psutil.Process(child.pid)
            except psutil.NoSuchProcess:
                return child.wait()
            owned.add(process)
            while child.poll() is None:
                try:
                    owned.update(process.children(recursive=True))
                except psutil.NoSuchProcess:
                    break
                # LaunchServices can parent the test app to launchd instead of xcodebuild.
                owned.update(_runtime_processes(runtime))
                rss = sum(_resident_bytes(candidate) for candidate in owned)
                peak_rss = max(peak_rss, rss)
                check_resources(rss, swap_before)
                if desktop:
                    check_desktop()
                if time.monotonic() - started > timeout:
                    msg = f"Workload exceeded {timeout:g}s: {command[0]}"
                    raise TimeoutError(msg)
                if profile is not None:
                    sampler = _sample(Path(profile), runtime, sampler)
                    if sampler is not None and sampler.poll() is None:
                        owned.add(psutil.Process(sampler.pid))
                time.sleep(0.5)
            return child.wait()
        finally:
            owned.update(_runtime_processes(runtime))
            stop_processes(owned)
            if sampler is not None:
                sampler.wait(timeout=5)
            sys.stdout.write(
                f"Sampled owned peak RSS: {peak_rss / _GIB:.2f} GiB; elapsed: {time.monotonic() - started:.1f}s\n"
            )


def _resident_bytes(process: psutil.Process) -> int:
    try:
        return process.memory_info().rss
    except psutil.NoSuchProcess:
        return 0


def _sample(profile: Path, runtime: Path, sampler: subprocess.Popen[bytes] | None) -> subprocess.Popen[bytes] | None:
    if sampler is None and (profile / "request").exists():
        requested: Final = psutil.Process(int((profile / "request").read_text().strip()))
        if requested not in _runtime_processes(runtime) or "--verify-ui" not in requested.cmdline():
            msg: Final = "Refusing to sample a process outside this verification run"
            raise RuntimeError(msg)
        with (profile / "sampler.log").open("wb") as stream:
            sampler = subprocess.Popen(  # ruff: ignore[subprocess-without-shell-equals-true] - PID must belong to this run's relocated verification executable.
                ["/usr/bin/sample", str(requested.pid), "3", "1", "-file", str(profile / "sample.pending")],
                stdout=stream,
                stderr=subprocess.STDOUT,
            )
    if sampler is not None and sampler.poll() == 0 and (profile / "sample.pending").exists():
        (profile / "sample.pending").rename(profile / "sample.txt")
    return sampler


def _runtime_processes(runtime: Path) -> set[psutil.Process]:
    return {
        process
        for process in psutil.process_iter(["exe", "uids"])
        if process.info["uids"] is not None
        and process.info["uids"].real == os.getuid()
        and process.info["exe"]
        and process.is_running()
        and Path(process.info["exe"]).is_relative_to(runtime)
    }


def check_resources(rss: int, swap_before: int) -> int:
    """Return normal/warning pressure; stop on critical pressure, excess RSS or swap growth."""
    pressure: Final = int(
        subprocess.check_output(["/usr/sbin/sysctl", "-n", "kern.memorystatus_vm_pressure_level"], text=True, timeout=5)
    )
    if pressure not in {1, 2} or rss > 4 * _GIB or psutil.swap_memory().used - swap_before > _GIB:
        msg = f"Stopping owned workload: pressure={pressure}, tree RSS={rss / _GIB:.2f} GiB"
        raise RuntimeError(msg)
    return pressure


def check_desktop() -> None:
    """Require the current user's unlocked console without requesting access or changing it."""
    # These are kernel diagnostic keys, not a public AppKit contract; an unknown schema must fail closed.
    state: Final = plistlib.loads(
        subprocess.check_output(["/usr/sbin/ioreg", "-n", "Root", "-d", "1", "-a"], timeout=5)
    )
    if (
        not isinstance(state, dict)
        or state.get("IOConsoleLocked") is not False
        or not isinstance(sessions := state.get("IOConsoleUsers"), list)
        or not any(
            isinstance(session, dict)
            and session.get("kCGSSessionUserIDKey") == os.getuid()
            and session.get("kCGSSessionOnConsoleKey") is True
            and session.get("kCGSessionLoginDoneKey") is True
            and not session.get("CGSSessionScreenIsLocked", False)
            for session in sessions
        )
    ):
        msg: Final = "Native UI tests need this user's unlocked desktop. Build-only checks remain available."
        raise RuntimeError(msg)


def stop_processes(processes: set[psutil.Process]) -> None:
    """Terminate process identities, escalating after three seconds without using process names."""
    for process in processes:
        _signal_process(process, signal.SIGTERM)
    _, alive = psutil.wait_procs(processes, timeout=3)
    for process in alive:
        _signal_process(process, signal.SIGKILL)
    _, alive = psutil.wait_procs(alive, timeout=3)
    if alive:
        msg: Final = f"Could not stop owned processes: {[process.pid for process in alive]}"
        raise RuntimeError(msg)


def _signal_process(process: psutil.Process, action: signal.Signals) -> None:
    try:
        process.send_signal(action)
    except psutil.NoSuchProcess:
        return


def _run(command: list[str], *, timeout: float, stdout: TextIO | None = None) -> None:
    subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] - fixed argv commands from this module; no shell interpolation.
        _executable(command),
        stdout=stdout,
        check=True,
        timeout=timeout,
    )


def _executable(command: list[str]) -> list[str]:
    if (executable := which(command[0])) is None:
        msg: Final = f"Required executable not found: {command[0]}"
        raise FileNotFoundError(msg)
    return [str(Path(executable).resolve()), *command[1:]]


def _interrupted(_signal: int, _frame: FrameType | None) -> None:
    raise KeyboardInterrupt


__all__ = [
    "UISuite",
    "check_desktop",
    "check_resources",
    "main",
    "run_bounded",
    "select_tests",
    "stop_processes",
    "validate_products",
    "validate_results",
]

if __name__ == "__main__":
    signal.signal(signal.SIGTERM, _interrupted)
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(130)
    except (OSError, TypeError, ValueError, RuntimeError, subprocess.SubprocessError, psutil.Error) as error:
        sys.stderr.write(f"{error}\n")
        sys.exit(1)
