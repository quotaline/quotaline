# /// script
# requires-python = ">=3.15"
# dependencies = []
# ///

"""Check distribution contracts before signing artifacts reach a release."""

import base64
import binascii
import plistlib
import re
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Final, cast

if TYPE_CHECKING:
    from collections.abc import Sequence


type _Plist = dict[str, str | bool | list[str]]


def main(arguments: Sequence[str]) -> None:
    """Validate packaged files, including embedded executables and app-group entitlements."""
    app_name, distribution, updater = arguments
    _require(condition=updater in {"required", "forbidden"}, message="updater policy must be required or forbidden")
    app: Final = Path(app_name)
    info: Final = cast("_Plist", plistlib.loads((app / "Contents/Info.plist").read_bytes()))
    _require(condition=info.get("TMBDistribution") == distribution, message=f"expected {distribution} distribution")
    _architectures(app)
    _updater(app, info, updater)
    signed: Final = _signatures(app, distribution)
    sys.stdout.write(
        f"verified {distribution} app: universal, dyld-valid, updater={updater}, "
        f"signing={'distribution' if signed else 'ad-hoc'}\n"
    )


def _architectures(app: Path) -> None:
    binaries = 0
    for path in sorted(app.rglob("*")):
        if not path.is_file() or "Mach-O" not in _command("/usr/bin/file", "-b", str(path)):
            continue
        binaries += 1
        architectures: Final = set(_command("/usr/bin/lipo", "-archs", str(path)).split())
        _require(condition={"arm64", "x86_64"} <= architectures, message=f"{path} is not universal arm64/x86_64")
        _command("/usr/bin/xcrun", "dyld_info", "-validate_only", str(path))
    _require(condition=binaries > 0, message=f"no Mach-O files found in {app}")


def _updater(app: Path, info: _Plist, policy: str) -> None:
    executable: Final = app / "Contents/MacOS" / str(info["CFBundleExecutable"])
    dependencies: Final = (
        _command("/usr/bin/xcrun", "otool", "-L", str(executable)),
        _command("/usr/bin/xcrun", "dyld_info", "-linked_dylibs", str(executable)),
    )
    framework: Final = app / "Contents/Frameworks/Sparkle.framework"
    keys: Final = {"SUFeedURL", "SUPublicEDKey", "SUEnableInstallerLauncherService"}
    enabled: Final = info.get("TMBSelfUpdateEnabled") == "YES"
    if policy == "forbidden":
        _require(condition=not enabled, message="self-update is enabled in distribution metadata")
        _require(
            condition=not framework.exists(), message="Sparkle.framework is embedded in a distribution that forbids it"
        )
        _require(
            condition=all("Sparkle.framework" not in text for text in dependencies),
            message="executable retains a Sparkle dependency",
        )
        _require(condition=not keys.intersection(info), message="Sparkle keys remain in distribution metadata")
        return
    _require(condition=framework.is_dir(), message="Direct build does not embed Sparkle.framework")
    _require(
        condition=all("Sparkle.framework" in text for text in dependencies),
        message="Direct executable has no Sparkle dependency",
    )
    _require(condition=keys <= info.keys(), message="Direct build is missing Sparkle metadata")
    public_key: Final = str(info["SUPublicEDKey"])
    _require(
        condition=bool(public_key) and "$(" not in public_key,
        message="Direct build has an empty or unexpanded SUPublicEDKey",
    )
    if enabled:
        try:
            decoded: Final = base64.b64decode(public_key, validate=True)
        except binascii.Error as error:
            sys.exit(f"invalid EdDSA public key: {error}")
        key_size: Final = 32
        _require(
            condition=len(decoded) == key_size and any(decoded),
            message="self-update needs a real 32-byte EdDSA public key",
        )


def _signatures(app: Path, distribution: str) -> bool:
    signature: Final = _command("/usr/bin/codesign", "-dv", "--verbose=4", str(app), check=False)
    team: Final = re.search(r"^TeamIdentifier=(.+)$", signature, flags=re.MULTILINE)
    if team is None or team[1] == "not set":
        return False
    store: Final = distribution == "App Store"
    expected_group: Final = "group.tech.bernat.quotaline" if store else f"{team[1]}.tech.bernat.quotaline"
    if not store:
        for bundle in (app, *sorted(app.rglob("*"))):
            if bundle.is_dir() and bundle.suffix in {".app", ".appex", ".xpc", ".framework"}:
                info: Final = _command("/usr/bin/codesign", "-dv", "--verbose=4", str(bundle))
                _require(
                    condition=bool(re.search(r"flags=.*\([^)]*runtime[^)]*\)", info)),
                    message=f"{bundle} lacks hardened runtime",
                )
                _command("/usr/bin/codesign", "--verify", "--strict", "--verbose=2", str(bundle))
    for bundle in (app, app / "Contents/PlugIns/QuotalineWidget.appex"):
        _require(condition=bundle.is_dir(), message=f"missing signed bundle: {bundle}")
        entitlements: Final = cast(
            "_Plist",
            plistlib.loads(_command("/usr/bin/codesign", "-d", "--entitlements", "-", "--xml", str(bundle)).encode()),
        )
        groups: Final = entitlements.get("com.apple.security.application-groups")
        _require(
            condition=isinstance(groups, list) and bool(groups) and groups[0] == expected_group,
            message=f"{bundle} uses wrong app group",
        )
        metadata: Final = cast("_Plist", plistlib.loads((bundle / "Contents/Info.plist").read_bytes()))
        _require(
            condition=metadata.get("QuotalineAppGroup") == expected_group,
            message=f"{bundle} metadata uses wrong app group",
        )
        _require(
            condition=entitlements.get("com.apple.security.app-sandbox", False) == (store or bundle.suffix == ".appex"),
            message=f"{bundle} uses wrong sandbox setting",
        )
    return True


def _command(*arguments: str, check: bool = True) -> str:
    result: Final = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] - fixed tools receive bundle paths as argv, never shell code.
        arguments, check=check, capture_output=True, text=True, timeout=120
    )
    return result.stdout if "--entitlements" in arguments else result.stdout + result.stderr


def _require(*, condition: bool, message: str) -> None:
    if not condition:
        sys.exit(message)


__all__ = ["main"]

if __name__ == "__main__":
    main(sys.argv[1:])
