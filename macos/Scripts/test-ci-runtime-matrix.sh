#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
probe="$(mktemp -d "${TMPDIR:-/tmp}/quotaline-runtime-matrix.XXXXXX")"
trap 'rm -rf "$probe"' EXIT
export GITHUB_OUTPUT="$probe/output" GITHUB_STEP_SUMMARY="$probe/summary"

release='{"tag_name":"xcode-27-arm64/20260907.0173","draft":false,"prerelease":false,"body":"- OS Version: macOS 27.0 (26A5406e)\r\n"}'
for state in complete prerelease draft draft-prerelease; do
  case "$state" in
    complete) mutation='.' ;;
    prerelease) mutation='.prerelease = true' ;;
    draft) mutation='.draft = true' ;;
    draft-prerelease) mutation='.draft = true | .prerelease = true' ;;
  esac
  : > "$GITHUB_OUTPUT"
  : > "$GITHUB_STEP_SUMMARY"
  jq "$mutation" <<< "$release" | bash Scripts/ci-runtime-matrix.sh > "$probe/result"
  jq -Rn --arg state "$state" '
    [inputs | capture("^(?<key>[^=]+)=(?<value>.*)$") | .value |= fromjson] | from_entries |
    (if $state == "complete" then [15,26,27] else [15,26] end) as $expected |
    if .runtimes == $expected
      and [.ui.include[]."runtime-major"] == $expected
      and [.release.include[]."runtime-major"] == [$expected[] | select(. >= 26)]
      and [.ui.include[].runner] == (["macos-15","macos-26"] + if $state == "complete" then ["xcode-27"] else [] end)
      and [.ui.include[]."minimum-xcode"] == ([26,26] + if $state == "complete" then [27] else [] end)
      and [.package.include[] | [."runtime-major",.mode]] == [$expected[] as $runtime | ["nonpresenting","native"][] | select($runtime != 26 or . != "native") | [$runtime,.]]
      and [.application.include[] | [."runtime-major",.scheme]] == [$expected[] | select(. >= 26) | . as $runtime | ["Direct","AppStore","Homebrew"][] | [$runtime,"Quotaline-" + .]]
      and all(.application.include[]; if .condition == "APPSTORE" then .sandbox == "YES" and .entitlements == "AppStore.entitlements" else .sandbox == "NO" and .entitlements == "Direct.entitlements" end)
      and all(.application.include[]; if .condition == "DIRECT" then .updater == "required" else .updater == "forbidden" end)
    then true else error("Incorrect CI matrices for \($state)") end
  ' "$GITHUB_OUTPUT" > /dev/null
  if [[ "$state" == complete ]]; then
    grep -Fq 'CI includes macOS 15, 26 and 27' "$GITHUB_STEP_SUMMARY"
  else
    grep -Fq 'macOS 27 jobs are omitted until deployment completes' "$GITHUB_STEP_SUMMARY"
  fi
done

for state in complete prerelease; do
  : > "$GITHUB_OUTPUT"
  jq --arg state "$state" '.prerelease = ($state == "prerelease")' <<< "$release" |
    FULL_MATRIX=0 bash Scripts/ci-runtime-matrix.sh > "$probe/result"
  jq -Rne '
    [inputs | capture("^(?<key>[^=]+)=(?<value>.*)$") | .value |= fromjson] | from_entries |
    .full == false and ."ui-shards" == 2 and [.ui.include[]."runtime-major"] == [26]
  ' "$GITHUB_OUTPUT" > /dev/null
done
: > "$GITHUB_OUTPUT"
bash Scripts/ci-runtime-matrix.sh <<< "$release" > "$probe/result"
jq -Rne '
  [inputs | capture("^(?<key>[^=]+)=(?<value>.*)$") | .value |= fromjson] | from_entries |
  .full == true and ."ui-shards" == 0
' "$GITHUB_OUTPUT" > /dev/null

for runtime in 15 26 27; do
  : > "$GITHUB_OUTPUT"
  DIAGNOSTIC_UI_RUNTIME="$runtime" bash Scripts/ci-runtime-matrix.sh <<< "$release" > "$probe/result"
  jq -Rne --argjson runtime "$runtime" '
    [inputs | capture("^(?<key>[^=]+)=(?<value>.*)$") | .value |= fromjson] | from_entries |
    .runtimes == [15,26,27] and [.ui.include[]."runtime-major"] == [$runtime]
  ' "$GITHUB_OUTPUT" > /dev/null
done
for state in complete prerelease; do
  for runtime in 0 15 26 27; do
    : > "$GITHUB_OUTPUT"
    jq --arg state "$state" '.prerelease = ($state == "prerelease")' <<< "$release" |
      UI_GROUP=performance DIAGNOSTIC_UI_RUNTIME="$runtime" bash Scripts/ci-runtime-matrix.sh > "$probe/result"
    jq -Rne --arg state "$state" '
      [inputs | capture("^(?<key>[^=]+)=(?<value>.*)$") | .value |= fromjson] | from_entries |
      [.ui.include[]."runtime-major"] == [if $state == "complete" then 27 else 26 end]
    ' "$GITHUB_OUTPUT" > /dev/null
  done
done

# shellcheck disable=SC2329 # the sourced runtime check calls these stubs
gh() { printf '%s\n' "$BENCHMARK_RELEASE"; }
# shellcheck disable=SC2329 # the sourced runtime check calls these stubs
sw_vers() { printf '%s.0\n' "$BENCHMARK_RUNTIME"; }
for state in complete prerelease; do
  BENCHMARK_RELEASE="$(jq --arg state "$state" '.prerelease = ($state == "prerelease")' <<< "$release")"
  for runtime in 15 26 27; do
    BENCHMARK_RUNTIME="$runtime"
    status=0
    (source Scripts/check-benchmark-runtime.sh) > "$probe/result" 2>&1 || status=$?
    if [[ "$state" == complete && "$runtime" == 27 || "$state" == prerelease && "$runtime" == 26 ]]; then
      test "$status" == 0
    else
      test "$status" == 1
      grep -Fq 'Benchmarks require macOS' "$probe/result"
    fi
  done
done
unset -f gh sw_vers

# 14 dropped with the minimum-OS bump, 13 never supported: both must stay rejected regardless of rollout state.
for runtime in 13 14 27; do
  : > "$GITHUB_OUTPUT"
  if jq '.prerelease = true' <<< "$release" | DIAGNOSTIC_UI_RUNTIME="$runtime" bash Scripts/ci-runtime-matrix.sh \
    > "$probe/error" 2>&1; then
    echo "Accepted an unavailable diagnostic runtime: $runtime" >&2
    exit 1
  fi
  test ! -s "$GITHUB_OUTPUT"
done

for mutation in 'del(.draft)' 'del(.prerelease)' '.prerelease = "false"' '.draft = 0' '.tag_name = "unrelated"' '.body = "- OS Version: macOS 26.5.2"' '.body = null' 'null' '[]'; do
  : > "$GITHUB_OUTPUT"
  : > "$GITHUB_STEP_SUMMARY"
  if jq "$mutation" <<< "$release" | bash Scripts/ci-runtime-matrix.sh > "$probe/error" 2>&1; then
    echo "Accepted invalid rollout metadata: $mutation" >&2
    exit 1
  fi
  test ! -s "$GITHUB_OUTPUT"
  test ! -s "$GITHUB_STEP_SUMMARY"
done
for invalid in '' '{'; do
  if bash Scripts/ci-runtime-matrix.sh <<< "$invalid" > "$probe/error" 2>&1; then
    echo "Accepted empty or malformed rollout metadata" >&2
    exit 1
  fi
  test ! -s "$GITHUB_OUTPUT"
done
echo "Runtime matrix tests passed."
