#!/usr/bin/env bash
set -euo pipefail

release="$(jq -ce '
  if .tag_name == "xcode-27-arm64/20260907.0173"
    and (.draft | type) == "boolean"
    and (.prerelease | type) == "boolean"
    and (.body | test("(?m)^- OS Version: macOS 27[. ]"))
  then . else error("Invalid macOS 27 rollout metadata") end
')"

matrices="$(jq -cn --argjson release "$release" \
  --argjson diagnostic "${DIAGNOSTIC_UI_RUNTIME:-0}" --arg group "${UI_GROUP:-}" \
  --argjson full "${FULL_MATRIX:-1}" '
  ($release.draft == false and $release.prerelease == false) as $ready |
  [
    {"runtime-major":15, "minimum-xcode":26, runner:"macos-15"},
    {"runtime-major":26, "minimum-xcode":26, runner:"macos-26"},
    {"runtime-major":27, "minimum-xcode":27, runner:"xcode-27"} | select(."runtime-major" != 27 or $ready) |
    . + {name: "macOS \(."runtime-major")"}
  ] as $runtimes |
  [
    {scheme:"Quotaline-Direct", target:"QuotalineDirect", configuration:"Release",
     distribution:"Direct", condition:"DIRECT", sandbox:"NO", entitlements:"Direct.entitlements", updater:"required"},
    {scheme:"Quotaline-AppStore", target:"QuotalineAppStore", configuration:"AppStore",
     distribution:"App Store", condition:"APPSTORE", sandbox:"YES", entitlements:"AppStore.entitlements", updater:"forbidden"},
    {scheme:"Quotaline-Homebrew", target:"QuotalineHomebrew", configuration:"Homebrew",
     distribution:"Homebrew", condition:"HOMEBREW", sandbox:"NO", entitlements:"Direct.entitlements", updater:"forbidden"}
  ] as $distributions |
  {
    runtimes: [$runtimes[]."runtime-major"],
    full: ($full == 1),
    "ui-shards": (if $full == 1 then 0 else 2 end),
    ui: {include: [$runtimes[] | select(
      if $group == "performance" then ."runtime-major" == ($runtimes | map(."runtime-major") | max)
      elif $diagnostic != 0 then ."runtime-major" == $diagnostic
      else $full == 1 or ."runtime-major" == 26 end)]},
    release: {include: [$runtimes[] | select(."runtime-major" >= 26)]},
    package: {include: [
      $runtimes[] | . + {mode:("nonpresenting", "native")} |
      select(."runtime-major" != 26 or .mode != "native") |
      .name += " / Xcode \(."minimum-xcode")"
    ]},
    application: {include: [$runtimes[] | select(."runtime-major" >= 26) | . + $distributions[]]}
  } | if .ui.include | length > 0 then . else error("Diagnostic runtime is not deployed") end
')"

jq -r 'to_entries[] | "\(.key)=\(.value | tojson)"' <<< "$matrices" >> "$GITHUB_OUTPUT"
if jq -e '.runtimes | index(27)' <<< "$matrices" > /dev/null; then
  message="GitHub completed the macOS 27 image rollout. CI includes macOS 15, 26 and 27 with runtime checks."
else
  message="GitHub has not completed the macOS 27 image rollout. CI requires macOS 15 and 26; macOS 27 jobs are omitted until deployment completes."
fi
echo "$message"
echo "$message" >> "$GITHUB_STEP_SUMMARY"
