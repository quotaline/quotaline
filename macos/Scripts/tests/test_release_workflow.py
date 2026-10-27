"""Exercise release reruns against local Git refs and recorded API responses."""

import json
import os
import shutil
import subprocess
from pathlib import Path
from textwrap import dedent
from typing import Final, cast

import pytest
import yaml


def workflow_step(workflow: str, job: str, name: str) -> dict[str, str | bool]:
    """Read the checked-in step so a workflow edit changes the test.

    :param workflow: Workflow filename.
    :param job: Job identifier.
    :param name: Step name.
    :returns: The workflow step.
    """
    document = yaml.safe_load((Path(__file__).resolve().parents[3] / ".github/workflows" / workflow).read_text())
    return next(step for step in document["jobs"][job]["steps"] if step.get("name") == name)


def run_step(step: dict[str, str | bool], directory: Path, **environment: str) -> subprocess.CompletedProcess[str]:
    """Execute a workflow step with local resources.

    :param step: Workflow step.
    :param directory: Disposable checkout.
    :param environment: GitHub context values.
    :returns: Captured process result.
    """
    return subprocess.run(  # noqa: S603 -- execute the checked-in workflow step, not external input.
        ["/bin/bash", "--noprofile", "--norc", "-eo", "pipefail", "-c", cast("str", step["run"])],
        cwd=directory,
        env={**os.environ, **environment},
        capture_output=True,
        text=True,
        check=False,
    )


def git(directory: Path, *arguments: str) -> str:
    """Use real Git refs during release tests.

    :param directory: Disposable repository.
    :param arguments: Git arguments.
    :returns: Captured stdout.
    """
    executable: Final = shutil.which("git")
    assert executable is not None
    result = subprocess.run(  # noqa: S603 -- arguments are fixed by these tests.
        [executable, "-C", str(directory), *arguments], check=True, capture_output=True, text=True
    )
    return result.stdout.strip()


@pytest.fixture
def checkout(tmp_path: Path) -> tuple[Path, Path, str]:
    """Keep release pushes off GitHub.

    :param tmp_path: Pytest temporary directory.
    :returns: Checkout, bare origin, and initial commit.
    """
    source: Final = tmp_path / "source"
    origin: Final = tmp_path / "origin.git"
    source.mkdir()
    git(tmp_path, "init", "--bare", str(origin))
    git(tmp_path, "init", "-b", "main", str(source))
    git(source, "config", "user.name", "Release Test")
    git(source, "config", "user.email", "release@example.invalid")
    (source / "gnome").mkdir()
    (source / "gnome/Cargo.toml").write_text('[package]\nname = "quotaline"\nversion = "1.2.3"\n')
    changelog: Final = source / "website/content/changelog/_index.md"
    changelog.parent.mkdir(parents=True)
    changelog.write_text("## 1.2.2 (2026-01-01)\n")
    git(source, "add", ".")
    git(source, "commit", "-m", "Before release")
    git(source, "remote", "add", "origin", str(origin))
    git(source, "push", "origin", "main")
    original = git(source, "rev-parse", "HEAD")
    return source, origin, original


def test_full_prepare_rerun_reuses_original_version(checkout: tuple[Path, Path, str], tmp_path: Path) -> None:
    """A later tag cannot advance the version of an existing release attempt."""
    source, _, _ = checkout
    git(source, "tag", "v1.2.3")
    tools: Final = tmp_path / "bin"
    tools.mkdir()
    (tools / "gh").write_text("#!/bin/sh\nprintf '42\\n'\n")
    (tools / "gh").chmod(0o755)
    output: Final = tmp_path / "output"
    result = run_step(
        workflow_step("prepare-release.yml", "assemble", "📩 Reuse this run's release patch"),
        source,
        PATH=f"{tools}:{os.environ['PATH']}",
        GITHUB_OUTPUT=str(output),
        GITHUB_REPOSITORY="quotaline/quotaline",
        GITHUB_RUN_ID="123",
    )
    assert result.returncode == 0, result.stderr
    assert output.read_text().splitlines() == ["version=v1.2.3", "artifact-id=42"]
    assert workflow_step("prepare-release.yml", "assemble", "🏷️ Select version from fragments")["if"] == (
        "steps.existing.outputs.artifact-id == ''"
    )


def test_push_only_rerun_reuses_published_commit(checkout: tuple[Path, Path, str], tmp_path: Path) -> None:
    """A completed remote push survives a failed-job rerun."""
    source, _, original = checkout
    changelog = source / "website/content/changelog/_index.md"
    changelog.write_text("## 1.2.3 (2026-01-02)\n" + changelog.read_text())
    git(source, "add", ".")
    git(source, "commit", "-m", "Prepare v1.2.3")
    published = git(source, "rev-parse", "HEAD")
    git(source, "push", "origin", "main")
    git(source, "reset", "--hard", original)
    output: Final = tmp_path / "output"
    result = run_step(
        workflow_step("prepare-release.yml", "push", "🔍 Inspect remote release commit"),
        source,
        GITHUB_SHA=original,
        GITHUB_OUTPUT=str(output),
    )
    assert result.returncode == 0, result.stderr
    assert output.read_text().strip() == f"published-commit={published}"
    result = run_step(
        workflow_step("prepare-release.yml", "push", "🔍 Reuse prepared release commit"),
        source,
        GITHUB_SHA=original,
        PUBLISHED_COMMIT=published,
    )
    assert result.returncode == 0, result.stderr
    assert git(source, "rev-parse", "HEAD") == published


def test_moved_tag_blocks_app_store_upload(checkout: tuple[Path, Path, str]) -> None:
    """A moved tag cannot replace the source verified before the upload."""
    source, _, original = checkout
    (source / "changed").write_text("other source\n")
    git(source, "add", ".")
    git(source, "commit", "-m", "Different source")
    git(source, "tag", "-a", "v1.2.3", "-m", "Moved")
    git(source, "push", "origin", "v1.2.3")
    result = run_step(
        workflow_step("release.yml", "app-store", "🔍 Verify tag before App Store upload"),
        source,
        TAG="v1.2.3",
        SOURCE_COMMIT=original,
    )
    assert result.returncode != 0
    assert "moved after source verification" in result.stderr


def test_tag_records_source_commit(checkout: tuple[Path, Path, str], tmp_path: Path) -> None:
    """The tagged commit becomes the commit every release job builds."""
    source, _, original = checkout
    output: Final = tmp_path / "output"
    result = run_step(
        workflow_step("release.yml", "verify-source", "⚙️ Record the tagged commit"),
        source,
        GITHUB_OUTPUT=str(output),
    )
    assert result.returncode == 0, result.stderr
    assert output.read_text() == f"sha={original}\n"


@pytest.mark.parametrize(
    ("started", "branch", "event", "job"),
    [
        (None, "v1.2.3", "push", "🚀 Mac App Store upload (v1.2.3)"),
        ("2026-01-02T10:00:00Z", "v1.2.3", "push", "🚀 Mac App Store upload (v1.2.3)"),
        ("2026-01-02T10:00:00Z", "main", "workflow_dispatch", "Mac App Store upload"),
    ],
    ids=["not-started", "same-tag", "legacy-dispatch"],
)
def test_app_store_prior_attempt_decision(
    tmp_path: Path, started: str | None, branch: str, event: str, job: str
) -> None:
    """A started upload requires manual reconciliation, including legacy dispatches."""
    tools: Final = tmp_path / "bin"
    tools.mkdir()
    runs = json.dumps({"workflow_runs": [{"id": 41, "head_branch": branch, "event": event}]})
    jobs = json.dumps(
        {
            "jobs": [
                {
                    "name": job,
                    "steps": [{"name": "🚀 Archive, export and upload", "started_at": started}],
                }
            ]
        }
    )
    (tools / "gh").write_text(
        dedent(
            f"""\
            #!/bin/sh
            case $* in
              *'/runs?'*) printf '%s\\n' '{runs}' ;;
              *) printf '%s\\n' '{jobs}' ;;
            esac
            """
        )
    )
    (tools / "gh").chmod(0o755)
    result = run_step(
        workflow_step("release.yml", "app-store", "🔍 Reject previous App Store upload attempt"),
        tmp_path,
        PATH=f"{tools}:{os.environ['PATH']}",
        GITHUB_REPOSITORY="quotaline/quotaline",
        GITHUB_RUN_ID="42",
        RUNNER_TEMP=str(tmp_path),
        TAG="v1.2.3",
    )
    assert (result.returncode != 0) == (started is not None)
    assert ("reconcile in App Store Connect" in result.stderr) == (started is not None)


def test_app_store_job_rerun_requires_manual_reconciliation(tmp_path: Path) -> None:
    """A rerun cannot turn an uncertain App Store upload green."""
    step = workflow_step("release.yml", "app-store", "🔍 Require App Store reconciliation before retry")
    assert "github.run_attempt > 1" in cast("str", step["if"])
    assert run_step(step, tmp_path).returncode != 0
