"""Exercise the local runner with synthetic bundles and disposable processes, without AppKit."""

import json
import os
import plistlib
import runpy
import signal
import subprocess
import sys
import tempfile
from itertools import chain, repeat
from pathlib import Path
from shutil import copytree, disk_usage
from typing import TYPE_CHECKING, Final
from unittest import TestCase

import psutil
import pytest

from Scripts.local_ui_tests import (
    check_desktop,
    check_resources,
    main,
    run_bounded,
    select_tests,
    stop_processes,
    validate_products,
    validate_results,
)

if TYPE_CHECKING:
    from typing import TextIO
    from unittest.mock import MagicMock

    from pytest_mock import MockerFixture

_ASSERT: Final = TestCase()
_TEST: Final = "QuotalineApplicationUITests/Suite/testExample"
_LAUNCH: Final = "QuotalineApplicationUITests/QuotalineApplicationUITests/testEveryTabExposesNamedControls"
_BENCHMARK: Final = "QuotalinePerformanceTests/PerformanceBenchmarks/testFirstSettingsClickFromFreshLaunch"
_BUILD_FAILURE: Final = 65
_UNATTENDED: Final = "This device DOES NOT REQUIRE user authentication to enable Automation Mode."


@pytest.fixture(autouse=True)  # ruff: ignore[pytest-fixture-autouse] - blocks live commands in every test
def commands(mocker: MockerFixture) -> MagicMock:
    """Keep project generation, signing and bundle export behind a subprocess boundary."""
    return mocker.patch("subprocess.run", autospec=True)


@pytest.fixture
def suite() -> str:
    """Use functional products unless a test requests the benchmark suite."""
    return "functional"


@pytest.fixture
def products(tmp_path: Path, suite: str) -> Path:
    """Avoid reading installed apps by creating a complete synthetic plan."""
    app: Final = tmp_path / "Products/Verification/QuotalineDirect.app/Contents"
    app.mkdir(parents=True)
    (app / "Info.plist").write_bytes(
        plistlib.dumps(
            {
                "CFBundleIdentifier": "tech.bernat.quotaline.verification",
                "TMBVerificationOnly": "YES",
            }
        )
    )
    target_name: Final = "QuotalinePerformanceTests" if suite == "performance" else "QuotalineApplicationUITests"
    identifier: Final = (
        "tech.bernat.quotaline.performance-tests.xctrunner"
        if suite == "performance"
        else "tech.bernat.quotaline.application-ui-tests.xctrunner"
    )
    runner: Final = tmp_path / f"Products/Verification/{target_name}-Runner.app"
    (runner / f"Contents/PlugIns/{target_name}.xctest").mkdir(parents=True)
    (runner / "Contents/Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier": identifier}))
    target: Final = {
        "BlueprintName": target_name,
        "TestHostBundleIdentifier": identifier,
        "UITargetAppPath": "__TESTROOT__/Verification/QuotalineDirect.app",
        "TestHostPath": f"__TESTROOT__/Verification/{target_name}-Runner.app",
        "TestBundlePath": f"__TESTHOST__/Contents/PlugIns/{target_name}.xctest",
        "IsUITestBundle": True,
        "PreferredScreenCaptureFormat": "screenshots",
        "SystemAttachmentLifetime": "keepNever",
    }
    (tmp_path / "Products/tests.xctestrun").write_bytes(
        plistlib.dumps({"TestConfigurations": [{"TestTargets": [target]}]})
    )
    return tmp_path / "Products"


def test_products_accept_self_contained_verification(products: Path) -> None:
    """The selected plan must point to the same products that passed validation."""
    (products / "Verification/current.app").symlink_to("QuotalineDirect.app")
    assert validate_products(products) == products / "tests.xctestrun"


@pytest.mark.parametrize("key", ["PreferredScreenCaptureFormat", "SystemAttachmentLifetime"])
def test_products_reject_automatic_capture(products: Path, key: str) -> None:
    """Automatic recordings must not replace the selected scenario's captures."""
    plan: Final = plistlib.loads((products / "tests.xctestrun").read_bytes())
    del plan["TestConfigurations"][0]["TestTargets"][0][key]
    (products / "tests.xctestrun").write_bytes(plistlib.dumps(plan))
    with pytest.raises(ValueError, match="disable automatic capture"):
        validate_products(products)


def test_products_reject_runner_identity_mismatch(products: Path) -> None:
    """A correct plan cannot authorize a runner bundle with a different identity."""
    (products / "Verification/QuotalineApplicationUITests-Runner.app/Contents/Info.plist").write_bytes(
        plistlib.dumps({"CFBundleIdentifier": "another.runner"})
    )
    with pytest.raises(ValueError, match="Runner bundle identity"):
        validate_products(products)


@pytest.mark.parametrize("valid_before_failure", [0, 1], ids=["application", "runner"])
def test_products_reject_invalid_signature(products: Path, commands: MagicMock, valid_before_failure: int) -> None:
    """Ad-hoc products still need an intact signature after relocation."""
    commands.side_effect = [None] * valid_before_failure + [subprocess.CalledProcessError(1, ["codesign", "--verify"])]
    with pytest.raises(subprocess.CalledProcessError):
        validate_products(products)


@pytest.mark.parametrize("count", [0, 2], ids=["missing", "ambiguous"])
def test_products_reject_plan_count(products: Path, count: int) -> None:
    """A stale second test plan must not alter which app Xcode runs."""
    if count == 0:
        (products / "tests.xctestrun").unlink()
    else:
        (products / "stale.xctestrun").write_bytes((products / "tests.xctestrun").read_bytes())
    with pytest.raises(ValueError, match="exactly one"):
        validate_products(products)


@pytest.mark.parametrize(
    "location",
    [
        "/Volumes/External/App",
        "/Users/example/Desktop/App",
        "/Users/example/Documents/App",
        "/Users/example/Downloads/App",
    ],
)
def test_products_reject_protected_paths(products: Path, location: str) -> None:
    """A relocated executable cannot retain a dependency on a protected volume."""
    (products / "tests.xctestrun").write_bytes(plistlib.dumps({"Path": location}))
    with pytest.raises(ValueError, match="Privacy-protected"):
        validate_products(products)


def test_products_reject_external_symlink(products: Path) -> None:
    """Framework links must stay inside the copied product tree."""
    (products / "escape").symlink_to(products.parent)
    with pytest.raises(ValueError, match="external symlink"):
        validate_products(products)


@pytest.mark.parametrize(
    "plan",
    [
        [],
        {},
        {"TestConfigurations": []},
        {"TestConfigurations": [False]},
        {"TestConfigurations": [{}]},
        {"TestConfigurations": [{"TestTargets": []}]},
    ],
    ids=["list", "missing", "empty", "wrong-configuration", "missing-target", "empty-target"],
)
def test_products_reject_invalid_structure(
    products: Path, plan: dict[str, list[dict[str, list[str]] | bool]] | list[str]
) -> None:
    """Malformed plans must fail before any native launch."""
    (products / "tests.xctestrun").write_bytes(plistlib.dumps(plan))
    with pytest.raises((TypeError, ValueError)):
        validate_products(products)


@pytest.mark.parametrize(
    "key", ["BlueprintName", "TestHostBundleIdentifier", "UITargetAppPath", "TestHostPath", "TestBundlePath"]
)
def test_products_reject_wrong_target(products: Path, key: str) -> None:
    """Matching the application name alone must not authorize another test host."""
    plan: Final = plistlib.loads((products / "tests.xctestrun").read_bytes())
    plan["TestConfigurations"][0]["TestTargets"][0][key] = "wrong"
    (products / "tests.xctestrun").write_bytes(plistlib.dumps(plan))
    with pytest.raises(ValueError, match=r"target|relocated"):
        validate_products(products)


@pytest.mark.parametrize(
    ("identifier", "verification"),
    [("tech.bernat.quotaline", "YES"), ("tech.bernat.quotaline.verification", "NO")],
    ids=["installed-identity", "debug-build"],
)
def test_products_reject_live_capable_app(products: Path, identifier: str, verification: str) -> None:
    """Neither production identity nor a demo-capable Debug build is verification-only."""
    (products / "Verification/QuotalineDirect.app/Contents/Info.plist").write_bytes(
        plistlib.dumps(
            {
                "CFBundleIdentifier": identifier,
                "TMBVerificationOnly": verification,
            }
        )
    )
    with pytest.raises(ValueError, match="verification-only"):
        validate_products(products)


@pytest.mark.parametrize("dependency", ["__TESTROOT__/../", "__TESTROOT__/missing", "/private/tmp/other-app"])
def test_products_reject_missing_or_escaped_dependency(products: Path, dependency: str) -> None:
    """Product-relative placeholders still need containment and existence checks."""
    plan: Final = plistlib.loads((products / "tests.xctestrun").read_bytes())
    plan["TestConfigurations"][0]["TestTargets"][0]["DependentProductPaths"] = [dependency]
    (products / "tests.xctestrun").write_bytes(plistlib.dumps(plan))
    with pytest.raises((ValueError, FileNotFoundError)):
        validate_products(products)


@pytest.mark.parametrize("selection", [[], [_TEST, _TEST], ["Target/Suite/testMissing"], ["$(touch unwanted)"]])
def test_selection_rejects_unknown_or_duplicate_methods(tmp_path: Path, selection: list[str]) -> None:
    """CLI input cannot expand the scope beyond declared test methods."""
    inventory: Final = tmp_path / "inventory.json"
    inventory.write_text(json.dumps({"group": ["Suite/testExample"]}))
    with pytest.raises(ValueError, match="Select existing"):
        select_tests(inventory, selection)


@pytest.mark.parametrize("selection", [["all"], [_TEST]])
def test_selection_preserves_exact_inventory(tmp_path: Path, selection: list[str]) -> None:
    """The all route must use the same inventory as a focused selection."""
    inventory: Final = tmp_path / "inventory.json"
    inventory.write_text(json.dumps({"group": ["Suite/testExample"]}))
    assert select_tests(inventory, selection) == [_TEST]


@pytest.mark.parametrize("suite", ["functional", "performance"])
def test_selection_keeps_suites_separate(tmp_path: Path, suite: str) -> None:
    """Selecting all must stay within the requested suite."""
    inventory: Final = tmp_path / "inventory.json"
    inventory.write_text(
        json.dumps(
            {
                "lifecycle": [_LAUNCH.split("/", 1)[1]],
                "performance": [_BENCHMARK.split("/", 1)[1]],
            }
        )
    )
    assert select_tests(inventory, ["all"], suite=suite) == [_BENCHMARK if suite == "performance" else _LAUNCH]


@pytest.mark.parametrize("suite", ["functional", "performance"])
def test_selection_rejects_the_other_suite(tmp_path: Path, suite: str) -> None:
    """A selector cannot opt into another suite."""
    inventory: Final = tmp_path / "inventory.json"
    inventory.write_text(
        json.dumps(
            {
                "lifecycle": [_LAUNCH.split("/", 1)[1]],
                "performance": [_BENCHMARK.split("/", 1)[1]],
            }
        )
    )
    with pytest.raises(ValueError, match="Select existing"):
        select_tests(inventory, [_LAUNCH if suite == "performance" else _BENCHMARK], suite=suite)


@pytest.mark.parametrize("results", [[], ["Skipped"], ["Failed"], ["Passed", "Passed"]])
def test_result_rejects_incomplete_or_retried_acceptance(tmp_path: Path, results: list[str]) -> None:
    """An Xcode success exit does not establish exact test coverage."""
    result: Final = tmp_path / "tests.json"
    result.write_text(
        json.dumps(
            {
                "testNodes": [
                    {"nodeType": "Test Case", "nodeIdentifier": "Suite/testExample()", "result": status}
                    for status in results
                ]
            }
        )
    )
    with pytest.raises(ValueError, match="exactly once"):
        validate_results(result, [_TEST])


def test_result_accepts_selected_pass(tmp_path: Path) -> None:
    """A passing result must identify the selected method, not another method's count."""
    result: Final = tmp_path / "tests.json"
    result.write_text(
        json.dumps(
            {"testNodes": [{"nodeType": "Test Case", "nodeIdentifier": "Suite/testExample()", "result": "Passed"}]}
        )
    )
    validate_results(result, [_TEST])
    with pytest.raises(ValueError, match="exactly once"):
        validate_results(result, ["QuotalineApplicationUITests/Suite/testOther"])


@pytest.mark.parametrize(
    ("pressure", "rss", "swap"),
    [(4, 0, 0), (0, 0, 0), (1, 5 * 1024**3, 0), (1, 0, 2 * 1024**3)],
    ids=["critical-pressure", "unknown-pressure", "owned-memory", "swap-growth"],
)
def test_resource_guard_rejects_unsafe_load(mocker: MockerFixture, pressure: int, rss: int, swap: int) -> None:
    """The build must stop before its resource usage threatens the desktop."""
    mocker.patch("subprocess.check_output", autospec=True, return_value=str(pressure))
    mocker.patch("psutil.swap_memory", autospec=True, return_value=psutil.swap_memory()._replace(used=swap))
    with pytest.raises(RuntimeError, match="Stopping owned workload"):
        check_resources(rss, 0)


@pytest.mark.parametrize("pressure", [1, 2], ids=["normal", "warning"])
def test_resource_guard_reports_noncritical_pressure(mocker: MockerFixture, pressure: int) -> None:
    """A warning alone does not justify aborting a bounded workload."""
    mocker.patch("subprocess.check_output", autospec=True, return_value=str(pressure))
    mocker.patch("psutil.swap_memory", autospec=True, return_value=psutil.swap_memory()._replace(used=0))
    assert check_resources(0, 0) == pressure


def test_existing_critical_pressure_prevents_start(tmp_path: Path, mocker: MockerFixture) -> None:
    """Do not add work when macOS has reached critical pressure."""
    mocker.patch("subprocess.check_output", autospec=True, return_value="4")
    launch: Final = mocker.patch("subprocess.Popen", autospec=True)
    with pytest.raises(RuntimeError, match="Stopping owned workload"):
        run_bounded([sys.executable, "-c", "pass"], tmp_path, timeout=3)
    launch.assert_not_called()


@pytest.mark.parametrize(
    ("initial", "subsequent"),
    [("2", "2"), ("1", "2"), ("1", "1")],
    ids=["existing-warning", "persistent-warning", "recovered-warning"],
)
def test_memory_warning_allows_completion(tmp_path: Path, mocker: MockerFixture, initial: str, subsequent: str) -> None:
    """Run past the removed two-second cutoff and preserve the child exit status."""
    mocker.patch("psutil.swap_memory", autospec=True, return_value=psutil.swap_memory()._replace(used=0))
    mocker.patch(
        "subprocess.check_output",
        autospec=True,
        side_effect=chain([initial, "2"], repeat(subsequent)),
    )
    status: Final = 23
    assert (
        run_bounded(
            [sys.executable, "-c", f"import time;time.sleep(2.6);raise SystemExit({status})"], tmp_path, timeout=10
        )
        == status
    )


@pytest.fixture
def unlocked_desktop() -> bytes:
    """Model console ownership without consulting the developer's session."""
    return plistlib.dumps(
        {
            "IOConsoleLocked": False,
            "IOConsoleUsers": [
                {
                    "kCGSSessionUserIDKey": os.getuid(),
                    "kCGSSessionOnConsoleKey": True,
                    "kCGSessionLoginDoneKey": True,
                }
            ],
        }
    )


@pytest.mark.parametrize(
    "state",
    [[], {}, {"IOConsoleLocked": True}, {"IOConsoleLocked": False}, {"IOConsoleLocked": False, "IOConsoleUsers": []}],
    ids=["invalid-schema", "missing-schema", "locked", "missing-users", "no-session"],
)
def test_desktop_rejects_unavailable_session(
    mocker: MockerFixture, state: dict[str, bool | list[str]] | list[str]
) -> None:
    """No native workload may run against an unavailable or unrecognized console."""
    mocker.patch("subprocess.check_output", autospec=True, return_value=plistlib.dumps(state))
    with pytest.raises(RuntimeError, match="unlocked desktop"):
        check_desktop()


@pytest.mark.parametrize("key", ["kCGSSessionUserIDKey", "kCGSSessionOnConsoleKey", "kCGSessionLoginDoneKey"])
def test_desktop_rejects_wrong_console(unlocked_desktop: bytes, mocker: MockerFixture, key: str) -> None:
    """A logged-in user must also own the console used by the test runner."""
    state: Final = plistlib.loads(unlocked_desktop)
    state["IOConsoleUsers"][0][key] = False
    mocker.patch("subprocess.check_output", autospec=True, return_value=plistlib.dumps(state))
    with pytest.raises(RuntimeError, match="unlocked desktop"):
        check_desktop()


def test_locked_desktop_fails_before_build(commands: MagicMock, mocker: MockerFixture) -> None:
    """A locked desktop must not consume a build before refusing input automation."""
    mocker.patch("subprocess.check_output", autospec=True, return_value=plistlib.dumps({"IOConsoleLocked": True}))
    with pytest.raises(RuntimeError, match="unlocked desktop"):
        main([_LAUNCH])
    commands.assert_not_called()


@pytest.mark.parametrize(
    "status",
    [
        "Automation Mode is disabled.\nThis device requires user authentication to enable Automation Mode.",
        "Automation Mode is enabled.\nThis device requires user authentication to enable Automation Mode.",
        "Automation Mode is enabled.",
        "",
    ],
    ids=["authentication-required", "temporary-authorization", "unknown-status", "empty-status"],
)
def test_automation_authentication_fails_before_build(
    commands: MagicMock, mocker: MockerFixture, unlocked_desktop: bytes, status: str
) -> None:
    """Temporary authorization can expire during a build and prompt on the next runner launch."""
    query: Final = mocker.patch("subprocess.check_output", autospec=True, side_effect=[unlocked_desktop, status])
    with pytest.raises(RuntimeError, match="administrator must configure it once"):
        main([_LAUNCH])
    commands.assert_not_called()
    query.assert_called_with(["/usr/bin/automationmodetool"], text=True, timeout=5, env=os.environ | {"LC_ALL": "C"})


@pytest.mark.parametrize(
    "error",
    [
        subprocess.CalledProcessError(1, ["/usr/bin/automationmodetool"]),
        subprocess.TimeoutExpired(["/usr/bin/automationmodetool"], 5),
        FileNotFoundError("automationmodetool"),
    ],
    ids=["query-failed", "query-timed-out", "tool-missing"],
)
def test_automation_query_failure_prevents_build(
    commands: MagicMock, mocker: MockerFixture, unlocked_desktop: bytes, error: OSError | subprocess.SubprocessError
) -> None:
    """An unreadable authorization state must not fall through to a native permission prompt."""
    mocker.patch("subprocess.check_output", autospec=True, side_effect=[unlocked_desktop, error])
    with pytest.raises(type(error)):
        main([_LAUNCH])
    commands.assert_not_called()


@pytest.fixture
def safe_resources(mocker: MockerFixture, unlocked_desktop: bytes) -> MagicMock:
    """Host load cannot make a synthetic runner test depend on unrelated processes."""
    query: Final = mocker.patch(
        "subprocess.check_output",
        autospec=True,
        side_effect=lambda command, **_kwargs: {
            "/usr/sbin/ioreg": unlocked_desktop,
            "/usr/sbin/sysctl": "1",
            "/usr/bin/automationmodetool": f"Automation Mode is disabled.\n{_UNATTENDED}",
        }[command[0]],
    )
    mocker.patch("psutil.swap_memory", autospec=True, return_value=psutil.swap_memory()._replace(used=0))
    return query


@pytest.mark.usefixtures("safe_resources")
@pytest.mark.parametrize("status", [0, 7])
def test_workload_preserves_exit_status(tmp_path: Path, status: int) -> None:
    """A failed command must not become successful during process cleanup."""
    assert run_bounded([sys.executable, "-c", f"raise SystemExit({status})"], tmp_path, timeout=3) == status


@pytest.mark.usefixtures("safe_resources")
def test_workload_timeout_kills_owned_process(tmp_path: Path) -> None:
    """Timeouts must reap the child instead of leaving an idle or blocked test app."""
    pid_file: Final = tmp_path / "pid"
    with pytest.raises(TimeoutError):
        run_bounded(
            [
                sys.executable,
                "-c",
                f"import os,time;open({str(pid_file)!r},'w').write(str(os.getpid()));time.sleep(30)",
            ],
            tmp_path,
            timeout=0.2,
        )
    assert not psutil.pid_exists(int(pid_file.read_text()))


def test_desktop_lock_during_run_stops_process(tmp_path: Path, mocker: MockerFixture, unlocked_desktop: bytes) -> None:
    """Losing the console after preflight must terminate the workload, not retry UI queries."""
    mocker.patch("psutil.swap_memory", autospec=True, return_value=psutil.swap_memory()._replace(used=0))
    mocker.patch(
        "subprocess.check_output",
        autospec=True,
        side_effect=["1", unlocked_desktop, _UNATTENDED, "1", plistlib.dumps({"IOConsoleLocked": True})],
    )
    launch: Final = mocker.spy(subprocess, "Popen")
    with pytest.raises(RuntimeError, match="unlocked desktop"):
        run_bounded(
            [sys.executable, "-c", "import time;time.sleep(30)"],
            tmp_path,
            timeout=3,
            environment={"TEST_RUNNER_QUOTALINE_TEST_DESKTOP": "local-verification"},
        )
    assert launch.spy_return.poll() is not None


def test_termination_preserves_unrelated_process() -> None:
    """Cleanup receives owned identities, not process-name matches."""
    with (
        subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"]) as owned,
        subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"]) as unrelated,
    ):
        try:
            stop_processes({psutil.Process(owned.pid)})
            assert unrelated.poll() is None
            assert owned.poll() is not None
        finally:
            unrelated.terminate()


def test_cli_invalid_selector_never_runs_build(commands: MagicMock) -> None:
    """The public entry point rejects mistakes before touching the Xcode project."""
    with pytest.raises(ValueError, match="Select existing"):
        main(["Not/A/Test"])
    commands.assert_not_called()


def test_result_only_route_cannot_build(tmp_path: Path, commands: MagicMock) -> None:
    """CI can use the local result contract without opting in to the local desktop route."""
    result: Final = tmp_path / "tests.json"
    result.write_text(
        json.dumps(
            {
                "testNodes": [
                    {"nodeType": "Test Case", "nodeIdentifier": _LAUNCH.split("/", 1)[1] + "()", "result": "Passed"}
                ]
            }
        )
    )
    assert main(["--check-results", str(result), _LAUNCH]) == 0
    commands.assert_not_called()


def test_long_text_selection_requires_recognizer_before_build(mocker: MockerFixture, commands: MagicMock) -> None:
    """A text audit must fail before building when its independent verifier is missing."""
    mocker.patch("Scripts.local_ui_tests.which", autospec=True, return_value=None)
    with pytest.raises(FileNotFoundError, match="tesseract"):
        main(["--prepare-only", "all"])
    commands.assert_not_called()


@pytest.fixture
def xcode_child(mocker: MockerFixture) -> MagicMock:
    """Replace the process boundary so entry-point tests cannot launch an application."""
    child: Final = mocker.create_autospec(subprocess.Popen, instance=True, pid=999999)
    child.wait.return_value = 0
    return child


@pytest.fixture
def simulated_xcode(
    products: Path,
    commands: MagicMock,
    mocker: MockerFixture,
    xcode_child: MagicMock,
    suite: str,
) -> list[list[str]]:
    """Exercise the complete entry point while Xcode returns disposable synthetic products."""
    mocker.patch("psutil.Process", autospec=True, side_effect=psutil.NoSuchProcess(999999))
    mocker.patch("psutil.process_iter", autospec=True, return_value=[])
    mocker.patch("pathlib.Path.home", autospec=True, return_value=products.parent)
    context: Final = mocker.create_autospec(subprocess.Popen, instance=True)
    context.__enter__.return_value = xcode_child
    entered: Final[list[list[str]]] = []

    def launch(command: list[str], **_options: dict[str, str] | bool | Path | None) -> MagicMock:
        entered.append(command)
        if "test-without-building" in command:
            Path(command[command.index("-resultBundlePath") + 1]).mkdir()
        return context

    def command_result(command: list[str], *, stdout: TextIO | None = None, **_options: bool | float) -> None:
        if command[0].endswith("/ditto"):
            copytree(
                products if command[1].endswith("Build/Products") else Path(command[1]), Path(command[2]), symlinks=True
            )
        if command[0].endswith("/xcrun") and stdout is not None:
            stdout.write(
                json.dumps(
                    {
                        "testNodes": [
                            {
                                "nodeType": "Test Case",
                                "nodeIdentifier": (_BENCHMARK if suite == "performance" else _LAUNCH).split("/", 1)[1]
                                + "()",
                                "result": "Passed",
                            }
                        ]
                    }
                )
            )

    commands.side_effect = command_result
    mocker.patch("subprocess.Popen", autospec=True, side_effect=launch)
    return entered


@pytest.mark.usefixtures("safe_resources")
@pytest.mark.parametrize("prepare", [False, True], ids=["run", "prepare-only"])
@pytest.mark.parametrize("suite", ["functional", "performance"])
def test_cli_complete_workflow(
    simulated_xcode: list[list[str]], suite: str, commands: MagicMock, *, prepare: bool
) -> None:
    """The build-only path must not create the process that drives native UI."""
    result: Final = main(
        (["--prepare-only"] if prepare else [])
        + [
            "--suite",
            suite,
            _BENCHMARK if suite == "performance" else _LAUNCH,
        ]
    )
    assert (result, len(simulated_xcode)) == (0, 1 if prepare else 2)
    assert all("Verification" in command or "test-without-building" in command for command in simulated_xcode)
    assert simulated_xcode[0][simulated_xcode[0].index("-scheme") + 1] == (
        "Quotaline-Benchmarks" if suite == "performance" else "Quotaline-Direct"
    )
    assert any(call.args[0][0].endswith("check-benchmark-runtime.sh") for call in commands.call_args_list) == (
        suite == "performance" and not prepare
    )


@pytest.mark.usefixtures("safe_resources", "simulated_xcode")
def test_cli_keeps_temporary_artifacts_in_workspace(mocker: MockerFixture) -> None:
    """Keep local UI artifacts on the workspace volume."""
    temporary_directory: Final = mocker.spy(tempfile, "TemporaryDirectory")
    assert main(["--prepare-only", _LAUNCH]) == 0
    expected: Final = Path(__file__).resolve().parents[3] / "target/tmp"
    assert any(call.kwargs.get("dir") == expected for call in temporary_directory.call_args_list)
    assert os.environ["TMPDIR"] == str(expected)


def test_benchmarks_reject_old_runtime_before_build(commands: MagicMock, mocker: MockerFixture) -> None:
    """Reject an older OS before allocating a compiler or test process."""
    commands.side_effect = subprocess.CalledProcessError(1, ["check-benchmark-runtime.sh"])
    launched: Final = mocker.patch("subprocess.Popen", autospec=True)
    with pytest.raises(subprocess.CalledProcessError):
        main(["--suite", "performance", _BENCHMARK])
    launched.assert_not_called()


@pytest.mark.parametrize("status", ["enabled", "disabled"])
def test_cli_accepts_unattended_automation(
    simulated_xcode: list[list[str]],
    xcode_child: MagicMock,
    safe_resources: MagicMock,
    unlocked_desktop: bytes,
    status: str,
) -> None:
    """Authentication policy, not the current session's enabled flag, determines unattended readiness."""
    safe_resources.side_effect = [unlocked_desktop, f"Automation Mode is {status}.\n{_UNATTENDED}", "1"]
    xcode_child.wait.return_value = _BUILD_FAILURE
    assert (main([_LAUNCH]), len(simulated_xcode)) == (_BUILD_FAILURE, 1)


def test_automation_authorization_change_prevents_runner_launch(
    tmp_path: Path, mocker: MockerFixture, unlocked_desktop: bytes
) -> None:
    """Recheck authorization after compilation before XCTest can present a password dialog."""
    mocker.patch("psutil.swap_memory", autospec=True, return_value=psutil.swap_memory()._replace(used=0))
    mocker.patch(
        "subprocess.check_output", autospec=True, side_effect=["1", unlocked_desktop, "authentication required"]
    )
    launch: Final = mocker.patch("subprocess.Popen", autospec=True)
    with pytest.raises(RuntimeError, match="unattended Automation Mode"):
        run_bounded(
            ["/usr/bin/true"],
            tmp_path,
            timeout=3,
            environment={"TEST_RUNNER_QUOTALINE_TEST_DESKTOP": "local-verification"},
        )
    launch.assert_not_called()


@pytest.mark.usefixtures("safe_resources")
def test_cli_bounds_compiler_backend_threads(simulated_xcode: list[list[str]]) -> None:
    """Xcode's job limit does not limit Swift's whole-module backend threads."""
    main(["--prepare-only", _LAUNCH])
    assert {
        "SWIFT_USE_PARALLEL_WHOLE_MODULE_OPTIMIZATION=NO",
        "OTHER_SWIFT_FLAGS=$(inherited) -num-threads 1",
    } <= set(simulated_xcode[0])


@pytest.mark.usefixtures("safe_resources", "simulated_xcode")
def test_cli_seals_runner_after_verifying_the_rebuilt_plugin(commands: MagicMock) -> None:
    """The runner's outer resource seal must include the current test plug-in."""
    main(["--prepare-only", _LAUNCH])
    signing: Final = [call.args[0] for call in commands.call_args_list if call.args[0][0] == "/usr/bin/codesign"]
    assert [command[1] for command in signing] == ["--verify", "--force", "--verify", "--verify"]
    assert signing[0][-1].endswith("Contents/PlugIns/QuotalineApplicationUITests.xctest")
    assert signing[1][-1].endswith("QuotalineApplicationUITests-Runner.app")
    assert "--deep" not in signing[1]
    assert "--preserve-metadata=identifier,entitlements,flags,runtime" in signing[1]


@pytest.mark.usefixtures("safe_resources", "simulated_xcode")
def test_cli_rejects_external_symlinks_before_signing(products: Path, commands: MagicMock) -> None:
    """Signing a staged test runner must not follow a link outside its products."""
    outside: Final = products.parent / "outside"
    outside.write_text("not part of the test app")
    (products / "Verification/QuotalineApplicationUITests-Runner.app/Contents/outside").symlink_to(outside)
    with pytest.raises(ValueError, match="external symlink"):
        main(["--prepare-only", _LAUNCH])
    assert not any("--force" in call.args[0] for call in commands.call_args_list)


@pytest.mark.usefixtures("safe_resources", "simulated_xcode")
def test_cli_rejects_low_internal_disk(mocker: MockerFixture) -> None:
    """Insufficient staging space must fail before starting native tests."""
    mocker.patch(
        "Scripts.local_ui_tests.disk_usage", autospec=True, return_value=disk_usage("/private/tmp")._replace(free=0)
    )
    with pytest.raises(RuntimeError, match="2 GiB"):
        main(["--prepare-only", _LAUNCH])


@pytest.mark.usefixtures("safe_resources")
def test_workload_rejects_missing_tool(tmp_path: Path) -> None:
    """A missing executable needs an error before a process or UI session exists."""
    with pytest.raises(FileNotFoundError, match="Required executable"):
        run_bounded(["quotaline-missing-tool"], tmp_path, timeout=1)


@pytest.mark.usefixtures("safe_resources", "simulated_xcode")
@pytest.mark.parametrize("status", [0, 1, 130], ids=["prepare", "invalid", "terminated"])
def test_command_line_exit_codes(mocker: MockerFixture, commands: MagicMock, status: int) -> None:
    """Shell callers need failures and cancellation to survive the command-line adapter."""
    mocker.patch(
        "sys.argv",
        ["local_ui_tests.py", "invalid"] if status == 1 else ["local_ui_tests.py", "--prepare-only", _LAUNCH],
    )
    if status == 128 + signal.SIGINT:
        commands.side_effect = lambda *_args, **_kwargs: signal.raise_signal(signal.SIGTERM)
    previous: Final = signal.getsignal(signal.SIGTERM)
    try:
        with pytest.raises(SystemExit) as raised:
            runpy.run_path("Scripts/local_ui_tests.py", run_name="__main__")
        assert raised.value.code == status
    finally:
        signal.signal(signal.SIGTERM, previous)


@pytest.mark.usefixtures("safe_resources", "simulated_xcode")
def test_build_failure_does_not_launch_tests(xcode_child: MagicMock) -> None:
    """A build failure must end the workflow without falling through to cached UI products."""
    xcode_child.wait.return_value = _BUILD_FAILURE
    assert main([_LAUNCH]) == _BUILD_FAILURE


@pytest.mark.usefixtures("safe_resources", "simulated_xcode")
def test_missing_results_fail_after_cleanup(mocker: MockerFixture) -> None:
    """An interrupted Xcode session cannot be reported as passing without results."""
    real_exists: Final = Path.exists
    mocker.patch(
        "pathlib.Path.exists",
        autospec=True,
        side_effect=lambda path: False if path.suffix == ".xcresult" else real_exists(path),
    )
    with pytest.raises(RuntimeError, match="result bundle"):
        main([_LAUNCH])


@pytest.mark.parametrize("survives", [False, True], ids=["killed", "cannot-stop"])
def test_termination_escalates(mocker: MockerFixture, *, survives: bool) -> None:
    """A blocked process must receive SIGKILL after its graceful shutdown deadline."""
    process: Final = mocker.create_autospec(psutil.Process, instance=True, pid=999999)
    mocker.patch(
        "psutil.wait_procs",
        autospec=True,
        side_effect=[([], [process]), ([], [process]) if survives else ([process], [])],
    )
    if survives:
        with pytest.raises(RuntimeError, match="Could not stop"):
            stop_processes({process})
    else:
        stop_processes({process})
    assert process.send_signal.call_args_list == [mocker.call(signal.SIGTERM), mocker.call(signal.SIGKILL)]


def test_termination_tolerates_already_exited_process(mocker: MockerFixture) -> None:
    """A process can exit between discovery and signalling without making cleanup fail."""
    process: Final = mocker.create_autospec(psutil.Process, instance=True)
    process.send_signal.side_effect = psutil.NoSuchProcess(999999)
    mocker.patch("psutil.wait_procs", autospec=True, return_value=([process], []))
    stop_processes({process})
    process.send_signal.assert_called_once_with(signal.SIGTERM)


@pytest.mark.usefixtures("safe_resources")
def test_workload_handles_parent_exit_during_discovery(tmp_path: Path, mocker: MockerFixture) -> None:
    """The child can exit after polling but before descendant enumeration."""
    process: Final = mocker.create_autospec(psutil.Process, instance=True)
    process.children.side_effect = psutil.NoSuchProcess(999999)
    mocker.patch("psutil.Process", autospec=True, return_value=process)
    mocker.patch("psutil.process_iter", autospec=True, return_value=[])
    mocker.patch("psutil.wait_procs", autospec=True, return_value=([process], []))
    expected: Final = 7
    assert run_bounded([sys.executable, "-c", f"raise SystemExit({expected})"], tmp_path, timeout=3) == expected


@pytest.mark.usefixtures("safe_resources")
@pytest.mark.parametrize("owned", [True, False], ids=["verification", "unrelated"])
def test_profile_samples_only_owned_verification(tmp_path: Path, mocker: MockerFixture, *, owned: bool) -> None:
    """A profiling request must never attach to the installed app or another process."""
    process: Final = mocker.create_autospec(psutil.Process, instance=True, pid=999998)
    application: Final = mocker.create_autospec(psutil.Process, instance=True, pid=999999)
    process.children.return_value = [application]
    process.memory_info.side_effect = psutil.NoSuchProcess(999998)
    application.memory_info.return_value.rss = 0
    application.cmdline.return_value = ["QuotalineDirect", "--verify-ui"]
    application.info = {
        "exe": str(tmp_path / "app") if owned else "/Applications/Quotaline.app/app",
        "uids": psutil.Process().uids(),
    }
    runner: Final = mocker.create_autospec(subprocess.Popen, instance=True, pid=999998)
    runner.poll.side_effect = [None, None, 0]
    runner.wait.return_value = 0
    sample: Final = mocker.create_autospec(subprocess.Popen, instance=True, pid=999997)
    sample.poll.side_effect = [None, None, 0, 0]
    sample.wait.return_value = 0
    context: Final = mocker.create_autospec(subprocess.Popen, instance=True)
    context.__enter__.return_value = runner
    (tmp_path / "request").write_text("999999")
    (tmp_path / "sample.pending").write_text("synthetic owned stacks")
    mocker.patch(
        "psutil.Process", autospec=True, side_effect=lambda pid: process if pid == process.pid else application
    )
    mocker.patch("psutil.process_iter", autospec=True, return_value=[application])
    mocker.patch("psutil.wait_procs", autospec=True, return_value=([process, application], []))
    launches: Final = mocker.patch("subprocess.Popen", autospec=True, side_effect=[context, sample])
    environment: Final = {"TEST_RUNNER_TMB_PROFILE_DIRECTORY": str(tmp_path)}
    if owned:
        assert run_bounded(["/usr/bin/true"], tmp_path, timeout=3, environment=environment) == 0
        assert (tmp_path / "sample.txt").read_text() == "synthetic owned stacks"
        assert launches.call_args_list[1].args[0][:2] == ["/usr/bin/sample", "999999"]
    else:
        with pytest.raises(RuntimeError, match="Refusing to sample"):
            run_bounded(["/usr/bin/true"], tmp_path, timeout=3, environment=environment)
        assert launches.call_count == 1
