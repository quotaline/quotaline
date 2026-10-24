set shell := ["sh", "-eu", "-c"]

uuid := "quotaline@bernat.tech"
home := env_var("HOME")
root := justfile_directory()
tmp := env_var_or_default("TMB_TMPDIR", root + "/target/tmp")
swift_bindings := root + "/target/swift-bindings"
xcframework := root + "/macos/Generated/QuotalineRustFFI.xcframework"
extension_build := root + "/target/extension"
export TMPDIR := tmp
nextest_profile := env_var_or_default("TMB_NEXTEST_PROFILE", "ci")
rust_scope := env_var_or_default("TMB_RUST_SCOPE", "workspace")
rust_packages := if rust_scope == "gnome" { "-p quotaline-gnome -p quotaline-gnome-adapter" } else { "--workspace" }
rust_coverage_ignore := "/rustc/|/target/|/[.]cargo/|/[.]rustup/|/macos/adapter/src/(credentials/(keychain|source_chain)|http/native|providers/antigravity/local|time/clock)_callback[.]rs|/macos/(adapter|platform)/tests/support/keychain[.]rs|/support/(http_transport|server_scanner)[.]rs" + if rust_scope == "gnome" { "|/crates/|/macos/" } else { "" }

# List available commands
default:
    @just --list --unsorted

_tmp:
    @mkdir -p {{ quote(tmp) }}

lines:
    #!/bin/sh
    set -eu
    count() {
        pattern=$1
        shift
        rg --files "$@" | rg "$pattern" | xargs wc -l | awk 'END {print $1}'
    }
    printf '%-25s %10s %10s\n' 'Component' 'Source' 'Tests'
    printf '%-25s %10s %10s\n' 'Shared Rust' \
        "$(count '^crates/([^/]+/)?[^/]+/src/' crates -g '*.rs')" \
        "$(count '^crates/([^/]+/)?[^/]+/tests/' crates -g '*.rs')"
    printf '%-25s %10s %10s\n' 'GNOME Rust' \
        "$(count '^gnome/(src|adapter/src)/' gnome -g '*.rs')" \
        "$(count '^gnome/(tests|adapter/tests)/' gnome -g '*.rs')"
    printf '%-25s %10s %10s\n' 'GNOME TypeScript' \
        "$(count '^gnome/extension/(src/|extension[.]ts$)' gnome/extension -g '*.ts')" \
        "$(count '^gnome/extension/tests/' gnome/extension/tests -g '*.ts')"
    printf '%-25s %10s %10s\n' 'macOS Rust' \
        "$(count '^macos/(adapter|platform)/src/' macos/adapter/src macos/platform/src -g '*.rs')" \
        "$(count '^macos/(adapter|platform)/tests/' macos/adapter/tests macos/platform/tests -g '*.rs')"
    printf '%-25s %10s %10s\n' 'macOS Swift core' \
        "$(count '^macos/src/QuotalineCore/' macos/src/QuotalineCore -g '*.swift')" \
        "$(count '^macos/tests/QuotalineCoreTests/' macos/tests/QuotalineCoreTests -g '*.swift')"
    printf '%-25s %10s %10s\n' 'macOS Swift UI' \
        "$(count '^macos/src/QuotalineUI/' macos/src/QuotalineUI -g '*.swift')" \
        "$(count '^macos/tests/(QuotalineUITests|ApplicationUI|UITestSupport|Performance)/' macos/tests -g '*.swift')"
    printf '%-25s %10s %10s\n' 'macOS Swift app/widgets' \
        "$(count '^macos/src/(Quotaline|QuotalineApp|QuotalineWidgetApp|QuotalineWidgets)/' macos/src -g '*.swift')" \
        "$(count '^macos/tests/QuotalineWidgetsTests/' macos/tests/QuotalineWidgetsTests -g '*.swift')"
    printf '%-25s %10s %10s\n' 'macOS Swift test support' '-' \
        "$(count '^macos/tests/QuotalineTestSupport/' macos/tests/QuotalineTestSupport -g '*.swift')"
    printf '%-25s %10s %10s\n' 'macOS Swift bridge test' '-' \
        "$(count '^macos/adapter/tests/.*[.]swift$' macos/adapter/tests -g '*.swift')"
    printf '%-25s %10s %10s\n' 'Generated Swift bindings' \
        "$(count '^macos/src/QuotalineRust/' macos/src/QuotalineRust -g '*.swift')" '-'

# Build the local service
build:
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo build --release --locked --bin quotaline-gnome

# Install the pinned TypeScript compiler, GNOME typings, and coverage tools
node-deps:
    mkdir -p {{ tmp }}
    if [ ! gnome/node_modules/.package-lock.json -nt gnome/package-lock.json ]; then \
      TMPDIR={{ tmp }} npm_config_cache={{ root }}/target/npm-cache npm ci --prefix {{ root }}/gnome --ignore-scripts --no-audit --no-fund; \
    fi

# Compile the GNOME extension into target/extension
extension-build: node-deps
    rm -rf {{ extension_build }}
    gnome/node_modules/.bin/tsc -p gnome/tsconfig.json
    cd gnome/extension && cp -R metadata.json stylesheet.css icons privacy.html notices.html provider-setup.html {{ extension_build }}

# Check GNOME formatting, lint, and types
lint-extension: node-deps
    npm --prefix gnome run check

biome *files: node-deps
    gnome/node_modules/.bin/biome check --write --error-on-warnings --no-errors-on-unmatched {{ files }}

eslint: node-deps
    cd gnome && node_modules/.bin/eslint extension eslint.config.mjs --max-warnings 0

typecheck-extension: node-deps
    npm --prefix gnome run typecheck

# Run TypeScript unit tests outside GNOME Shell
test-js: extension-build
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} node --test gnome/extension/tests/*.test.ts gnome/extension/tests/support/*.test.ts

# Run Rust and GNOME Shell tests with process isolation
test: test-js
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo nextest run --locked --workspace --features fixture-api --profile {{ nextest_profile }}

# Run tests that need no GNOME session
test-unit: test-js
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo nextest run --locked --workspace --features fixture-api --profile {{ nextest_profile }} -E 'not binary(gnome_shell)'

focus filter: extension-build
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo nextest run --locked --features fixture-api --profile {{ nextest_profile }} -E '{{ filter }}'

# Format Rust sources
fmt:
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo fmt --all

# Check Rust formatting
format-check:
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo fmt {{ if rust_scope == "gnome" { rust_packages } else { "--all" } }} --check

# Check every Cargo target
check:
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo check --locked {{ rust_packages }} --all-targets

# Check Clippy warnings
clippy:
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo clippy --locked {{ rust_packages }} --all-targets -- -D warnings

lint-workflow:
    jactionlint .github/workflows/*.yml

# Run the source checks
lint: format-check check clippy lint-workflow lint-extension

# Run the isolated GNOME Shell integration test
smoke: extension-build
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo nextest run --locked --features fixture-api --profile {{ nextest_profile }} --test gnome_shell

# Refresh the main visual review gallery
screenshots: extension-build
    mkdir -p {{ tmp }}
    TMB_SCREENSHOT_ALL=1 TMB_VIRTUAL_MONITOR=1280x2400 TMPDIR={{ tmp }} cargo nextest run --locked --features fixture-api --profile default --test gnome_shell -j 1 -E 'test(demo_usage_and_history_reach_shell) | test(demo_settings_reach_shell) | test(demo_formats_and_lifecycle_reach_shell)'

# Run both Rust and Shell line and branch coverage gates
coverage:
    cargo llvm-cov clean --workspace
    just coverage-shell-setup
    just coverage-rust-run 'all()'
    TMPDIR={{ tmp }} node gnome/extension/tests/support/coverage.ts report
    jq -e '.totals | (.lines.total > 0 and .lines.covered == .lines.total and .branches.total > 0 and .branches.covered == .branches.total)' target/rust-coverage-report.json

coverage-rust:
    just coverage-rust-run 'not binary(gnome_shell)'

# Storage, history, and credential sources reach full coverage through the platform suites that `coverage` runs.
coverage-crates:
    cargo llvm-cov clean --workspace
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} NEXTEST_STATUS_LEVEL=fail NEXTEST_SHOW_PROGRESS=none RUSTC_BOOTSTRAP=1 cargo llvm-cov nextest --locked $(for manifest in crates/*/Cargo.toml crates/*/*/Cargo.toml; do crate=$(dirname "$manifest"); case "$(basename "$crate")" in credential-sources|history|storage) ;; *) printf -- '-p quotaline-%s ' "$(basename "$crate")" ;; esac; done) --branch --json --no-default-ignore-filename-regex --ignore-filename-regex '{{ rust_coverage_ignore }}' --profile {{ nextest_profile }} --output-path target/crates-coverage-detailed.json
    jq -e '[.data[0].files[] | select(.filename | test("/crates/([^/]+/)?[^/]+/src/") and (test("/crates/[^/]+/(credential-sources|history|storage)/") | not))] | length > 0 and all(.[]; .summary.lines.count > 0 and .summary.lines.covered == .summary.lines.count and .summary.branches.covered == .summary.branches.count)' target/crates-coverage-detailed.json

coverage-macos-ffi:
    # UniFFI emits an unused Rust deserializer for the Swift callback; its ABI is exercised by the callback test.
    test $(wc -l < macos/adapter/src/credentials/keychain_callback.rs) -le 8
    test $(wc -l < macos/adapter/src/credentials/source_chain_callback.rs) -le 8
    test $(wc -l < macos/adapter/src/http/native_callback.rs) -le 11
    # Mockall's generated fallback branches are outside the behavior checked by the discovery tests.
    test $(wc -l < macos/adapter/tests/support/keychain.rs) -le 9
    test $(wc -l < macos/adapter/tests/support/http_transport.rs) -le 11
    test $(wc -l < macos/adapter/src/providers/antigravity/local_callback.rs) -le 8
    test $(wc -l < macos/adapter/src/time/clock_callback.rs) -le 10
    test $(wc -l < macos/adapter/tests/support/server_scanner.rs) -le 8
    test $(wc -l < macos/platform/tests/support/keychain.rs) -le 9
    cargo llvm-cov clean --workspace
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} NEXTEST_STATUS_LEVEL=fail NEXTEST_SHOW_PROGRESS=none RUSTC_BOOTSTRAP=1 cargo llvm-cov nextest --locked -p quotaline-macos-ffi -p quotaline-macos-platform --branch --json --no-default-ignore-filename-regex --ignore-filename-regex '{{ rust_coverage_ignore }}' --profile {{ nextest_profile }} --output-path target/macos-ffi-coverage-detailed.json
    jq -e '[.data[0].files[] | select(.filename | test("/macos/(adapter|platform)/src/"))] | length > 0 and all(.[]; .summary.lines.count > 0 and .summary.lines.covered == .summary.lines.count and .summary.branches.covered == .summary.branches.count)' target/macos-ffi-coverage-detailed.json

macos-bindings:
    mkdir -p {{ tmp }}
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    rm -rf {{ swift_bindings }}/Sources {{ swift_bindings }}/Headers {{ swift_bindings }}/Clean
    mkdir -p {{ root }}/target/swift-bindings/Sources {{ root }}/target/swift-bindings/Headers {{ root }}/macos/Generated
    MACOSX_DEPLOYMENT_TARGET=14.0 CARGO_TARGET_DIR={{ root }}/target/macos-rust cargo build --release --locked -p quotaline-macos-ffi --target aarch64-apple-darwin
    MACOSX_DEPLOYMENT_TARGET=14.0 CARGO_TARGET_DIR={{ root }}/target/macos-rust cargo build --release --locked -p quotaline-macos-ffi --target x86_64-apple-darwin
    CARGO_TARGET_DIR={{ root }}/target/uniffi-install cargo install --locked --version 0.32.2 --features cli --bin uniffi-bindgen-swift --root {{ root }}/target/uniffi-tools uniffi
    {{ root }}/target/uniffi-tools/bin/uniffi-bindgen-swift --config {{ root }}/macos/uniffi.toml --swift-sources {{ root }}/target/macos-rust/aarch64-apple-darwin/release/libquotaline_macos_ffi.a {{ root }}/target/swift-bindings/Sources
    {{ root }}/target/uniffi-tools/bin/uniffi-bindgen-swift --headers {{ root }}/target/macos-rust/aarch64-apple-darwin/release/libquotaline_macos_ffi.a {{ root }}/target/swift-bindings/Headers
    {{ root }}/target/uniffi-tools/bin/uniffi-bindgen-swift --modulemap --module-name quotaline_macos_ffiFFI --modulemap-filename module.modulemap {{ root }}/target/macos-rust/aarch64-apple-darwin/release/libquotaline_macos_ffi.a {{ root }}/target/swift-bindings/Headers
    lipo -create {{ root }}/target/macos-rust/aarch64-apple-darwin/release/libquotaline_macos_ffi.a {{ root }}/target/macos-rust/x86_64-apple-darwin/release/libquotaline_macos_ffi.a -output {{ root }}/target/swift-bindings/libquotaline_macos_ffi.a
    if ! cmp -s {{ swift_bindings }}/libquotaline_macos_ffi.a \
      {{ xcframework }}/macos-arm64_x86_64/libquotaline_macos_ffi.a \
      || ! diff -qr {{ swift_bindings }}/Headers {{ xcframework }}/macos-arm64_x86_64/Headers >/dev/null; then \
      rm -rf {{ xcframework }}; \
      xcodebuild -create-xcframework -library {{ swift_bindings }}/libquotaline_macos_ffi.a \
        -headers {{ swift_bindings }}/Headers -output {{ xcframework }}; \
    fi
    mkdir -p {{ root }}/macos/src/QuotalineRust {{ swift_bindings }}/Clean
    for source in {{ swift_bindings }}/Sources/*.swift; do \
      name=$(basename "$source"); \
      tail -n +4 "$source" | sed 's/[[:blank:]]*$//' > {{ swift_bindings }}/Clean/"$name"; \
      if ! cmp -s {{ swift_bindings }}/Clean/"$name" {{ root }}/macos/src/QuotalineRust/"$name"; then \
        cp {{ swift_bindings }}/Clean/"$name" {{ root }}/macos/src/QuotalineRust/"$name"; \
      fi; \
    done

macos-bridge-test: macos-bindings macos-bridge-verify

macos-bridge-verify:
    swiftc -swift-version 6 -target arm64-apple-macos14.0 -sdk "$(xcrun --sdk macosx --show-sdk-path)" -I {{ root }}/target/swift-bindings/Headers {{ root }}/target/swift-bindings/Sources/*.swift {{ root }}/macos/adapter/tests/SwiftBridge.swift {{ root }}/target/macos-rust/aarch64-apple-darwin/release/libquotaline_macos_ffi.a -o {{ root }}/target/swift-bindings/swift-bridge-test
    {{ root }}/target/swift-bindings/swift-bridge-test

coverage-gnome:
    just coverage-shell-setup
    just coverage-rust-run 'binary(gnome_shell)'
    TMPDIR={{ tmp }} node gnome/extension/tests/support/coverage.ts report

coverage-merge:
    TMPDIR={{ tmp }} node gnome/extension/tests/support/coverage.ts merge-rust target/coverage-rust/rust-coverage-report.json target/coverage-gnome/rust-coverage-report.json target/rust-coverage-report.json

coverage-shell-setup: extension-build
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} node gnome/extension/tests/support/coverage.ts prepare
    TMPDIR={{ tmp }} TMB_EXTENSION_DIR={{ root }}/target/js-coverage/extension TMB_JS_COVERAGE_DIR={{ root }}/target/js-coverage/runs node --test gnome/extension/tests/*.test.ts gnome/extension/tests/support/*.test.ts

coverage-rust-run filter:
    test $(wc -l < macos/adapter/src/credentials/keychain_callback.rs) -le 8
    test $(wc -l < macos/adapter/src/credentials/source_chain_callback.rs) -le 8
    test $(wc -l < macos/adapter/tests/support/keychain.rs) -le 9
    test $(wc -l < macos/platform/tests/support/keychain.rs) -le 9
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} TMB_EXTENSION_DIR={{ root }}/target/js-coverage/extension TMB_JS_COVERAGE_DIR={{ root }}/target/js-coverage/runs NEXTEST_STATUS_LEVEL=fail NEXTEST_SHOW_PROGRESS=none RUSTC_BOOTSTRAP=1 cargo llvm-cov nextest {{ rust_packages }} --locked --features fixture-api --branch --json --no-default-ignore-filename-regex --ignore-filename-regex '{{ rust_coverage_ignore }}' --profile {{ nextest_profile }} --output-path target/coverage-detailed.json -E '{{ filter }}'
    jq -r '.data[0].totals | "LLVM raw lines: \(.lines.covered)/\(.lines.count)\nLLVM raw branches: \(.branches.covered)/\(.branches.count)"' target/coverage-detailed.json
    TMPDIR={{ tmp }} RUSTC_BOOTSTRAP=1 cargo llvm-cov report --branch --lcov --no-default-ignore-filename-regex --ignore-filename-regex '{{ rust_coverage_ignore }}' --output-path target/coverage.lcov
    TMPDIR={{ tmp }} node gnome/extension/tests/support/coverage.ts report-rust

coverage-focus filter: extension-build
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} RUSTC_BOOTSTRAP=1 cargo llvm-cov nextest --no-clean --workspace --locked --features fixture-api --branch --json --summary-only --no-default-ignore-filename-regex --ignore-filename-regex '{{ rust_coverage_ignore }}' --profile {{ nextest_profile }} --output-path target/coverage-incremental.json -E '{{ filter }}'
    jq -r '.data[0].totals | "lines: \(.lines.covered)/\(.lines.count)\nbranches: \(.branches.covered)/\(.branches.count)"' target/coverage-incremental.json

# Measure Shell JavaScript line and branch coverage in headless GNOME
coverage-js:
    just coverage-shell-setup
    TMPDIR={{ tmp }} TMB_EXTENSION_DIR={{ root }}/target/js-coverage/extension TMB_JS_COVERAGE_DIR={{ root }}/target/js-coverage/runs NEXTEST_STATUS_LEVEL=fail NEXTEST_SHOW_PROGRESS=none cargo nextest run --locked --features fixture-api --profile {{ nextest_profile }} --test gnome_shell
    TMPDIR={{ tmp }} node gnome/extension/tests/support/coverage.ts report

# Check Markdown formatting
lint-docs:
    mkdir -p {{ tmp }} {{ root }}/target/cache
    TMPDIR={{ tmp }} XDG_CACHE_HOME={{ root }}/target/cache prek run mdformat --files README.md macos/README.md website/content/*.md website/content/*/*.md website/content/*/*/*.md

site:
    hugo --source website --minify

site-check:
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} node --test website/tests/*.test.mjs

site-serve:
    hugo server --source website

site-readthedocs:
    test -n "${READTHEDOCS_CANONICAL_URL:-}"
    test -n "${READTHEDOCS_OUTPUT:-}"
    mkdir -p "$READTHEDOCS_OUTPUT/html"
    hugo --source website --gc --minify --baseURL "$READTHEDOCS_CANONICAL_URL" --destination "$READTHEDOCS_OUTPUT/html"

# Check Rust dependencies
lint-deps:
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} cargo metadata --locked --format-version 1 --manifest-path {{ root }}/Cargo.toml > {{ root }}/target/deny-metadata.json
    TMPDIR={{ tmp }} cargo-deny --manifest-path {{ root }}/Cargo.toml --config {{ root }}/deny.toml fetch db std-replacement
    TMPDIR={{ tmp }} cargo-deny --manifest-path {{ root }}/Cargo.toml --config {{ root }}/deny.toml --metadata-path {{ root }}/target/deny-metadata.json check

# Run local repository hooks
pre-commit:
    mkdir -p {{ tmp }} {{ root }}/target/cache
    {{ if os() == "macos" { "" } else { "SKIP=swift-format,swiftlint" } }} TMPDIR={{ tmp }} XDG_CACHE_HOME={{ root }}/target/cache prek run --config {{ root }}/.pre-commit-config.yaml --all-files

# Run source checks without headless GNOME tests
ci-source: lint lint-docs lint-deps test-unit

# Run the full local release gate
ci: lint lint-docs lint-deps coverage coverage-crates coverage-macos-ffi

# Package the GNOME extension
pack: extension-build
    mkdir -p {{ tmp }}
    rm -rf target/extension-stage
    cp -LR {{ extension_build }} target/extension-stage
    rm -rf target/extension-stage/tests
    TMPDIR={{ tmp }} gnome-extensions pack target/extension-stage --out-dir target --force --extra-source=src --extra-source=icons --extra-source=privacy.html --extra-source=notices.html --extra-source=provider-setup.html

release-package: build pack
    rm -rf target/release-image
    mkdir -p target/release-image/bin target/release-image/extension target/release-image/packaging
    install -m755 target/release/quotaline-gnome target/release-image/bin/quotaline-gnome
    install -m644 target/{{ uuid }}.shell-extension.zip target/release-image/extension/{{ uuid }}.shell-extension.zip
    install -m644 gnome/packaging/quotaline-gnome.service gnome/packaging/tech.bernat.Quotaline.Gnome.service.in target/release-image/packaging
    install -m644 README.md LICENSE target/release-image
    tar -C target/release-image --sort=name --mtime='@0' --owner=0 --group=0 --numeric-owner -cf - . | gzip -n > target/quotaline-gnome-linux-x86_64.tar.gz
    unzip -t target/{{ uuid }}.shell-extension.zip
    cd target && sha256sum quotaline-gnome-linux-x86_64.tar.gz > quotaline-gnome-linux-x86_64.tar.gz.sha256
    just verify-release-package

verify-release-package:
    test -s target/quotaline-gnome-linux-x86_64.tar.gz
    tar -tzf target/quotaline-gnome-linux-x86_64.tar.gz
    cd target && sha256sum --check quotaline-gnome-linux-x86_64.tar.gz.sha256

# Build and install only into this user account
install: build pack _uninstall-legacy
    install -Dm755 target/release/quotaline-gnome {{ home }}/.local/libexec/quotaline-gnome
    install -Dm644 gnome/packaging/quotaline-gnome.service {{ home }}/.config/systemd/user/quotaline-gnome.service
    mkdir -p {{ home }}/.local/share/dbus-1/services
    sed 's|@BIN@|{{ home }}/.local/libexec/quotaline-gnome|g' gnome/packaging/tech.bernat.Quotaline.Gnome.service.in > {{ home }}/.local/share/dbus-1/services/tech.bernat.Quotaline.Gnome.service
    TMPDIR={{ tmp }} gnome-extensions install --force target/{{ uuid }}.shell-extension.zip
    systemctl --user daemon-reload
    systemctl --user disable quotaline-gnome.service
    systemctl --user restart quotaline-gnome.service
    gnome-extensions disable {{ uuid }} 2>/dev/null || true
    gjs -c 'const Gio = imports.gi.Gio; const settings = new Gio.Settings({schema_id: "org.gnome.shell"}); \
      settings.set_strv("disabled-extensions", settings.get_strv("disabled-extensions").filter(item => item !== "{{ uuid }}")); \
      settings.set_strv("enabled-extensions", [...new Set([...settings.get_strv("enabled-extensions"), "{{ uuid }}"])]); \
      Gio.Settings.sync()'
    if ! gnome-extensions info {{ uuid }} >/dev/null 2>&1; then \
      echo 'GNOME Shell has not discovered Quotaline yet. It will load at your next login.'; \
    fi
    @echo 'Installed locally. Use just screenshots for visual checks, or log out and back in to reload the login Shell.'

# The project shipped as Token Menu Bar before the rename; an upgrade must not leave a second panel item behind.
_uninstall-legacy:
    gjs -c 'const Gio = imports.gi.Gio; const settings = new Gio.Settings({schema_id: "org.gnome.shell"}); settings.set_strv("enabled-extensions", settings.get_strv("enabled-extensions").filter(item => item !== "token-menu-bar@bernat.tech"))'
    gnome-extensions uninstall token-menu-bar@bernat.tech >/dev/null 2>&1 || true
    rm -rf {{ home }}/.local/share/gnome-shell/extensions/token-menu-bar@bernat.tech
    systemctl --user disable --now token-menu-bar-gnome.service 2>/dev/null || true
    rm -f {{ home }}/.config/systemd/user/token-menu-bar-gnome.service {{ home }}/.local/share/dbus-1/services/dev.tox.TokenMenuBar.Gnome.service {{ home }}/.local/libexec/token-menu-bar-gnome

# Open the installed extension in a nested GNOME Shell window for manual inspection
manual-preview: install
    @echo 'Opening a nested GNOME Shell window. Close the window to exit.'
    TMPDIR={{ tmp }} XREMAP_GNOME_SOCKET={{ root }}/target/xremap-preview.sock dbus-run-session -- gnome-shell --devkit --wayland > {{ root }}/target/preview.log 2>&1

# Try the current build in a nested GNOME Shell window with demo data; your session, settings, and history stay untouched
demo: build pack
    #!/bin/sh
    set -eu
    if [ ! -x /usr/libexec/mutter-devkit ]; then
        echo 'The nested window needs mutter-devkit: sudo dnf install mutter-devkit' >&2
        exit 1
    fi
    host_runtime="${XDG_RUNTIME_DIR:?run this from your desktop session}"
    host_display="$host_runtime/${WAYLAND_DISPLAY:?run this from your desktop session}"
    sandbox={{ root }}/target/demo
    rm -rf "$sandbox"
    mkdir -p "$sandbox/home" "$sandbox/config" "$sandbox/cache" "$sandbox/runtime" \
        "$sandbox/data/gnome-shell/extensions/{{ uuid }}"
    chmod 700 "$sandbox/runtime"
    unzip -q {{ root }}/target/{{ uuid }}.shell-extension.zip -d "$sandbox/data/gnome-shell/extensions/{{ uuid }}"
    # Leftover helpers the nested Shell starts are found by the sandbox path in their environment.
    stop_sandbox() {
        for process in $(pgrep -u "$(id -u)"); do
            grep -qa "$sandbox" "/proc/$process/environ" 2>/dev/null && kill -9 "$process" 2>/dev/null || true
        done
    }
    trap stop_sandbox EXIT
    echo 'Opening a nested GNOME Shell with demo data. Close the window to stop it; logs go to target/demo/.'
    # Its own bus, runtime, data, and config directories keep the demo away from the desktop session; only the window
    # and the PipeWire stream it is drawn from are shared. Without an X display, login session, or system autostart
    # entries, desktop services such as remote access servers cannot mistake it for the real session.
    env -u DISPLAY -u XAUTHORITY -u SESSION_MANAGER -u XDG_SESSION_ID -u XDG_SEAT -u XDG_VTNR -u XDG_SESSION_CLASS \
        HOME="$sandbox/home" XDG_CONFIG_HOME="$sandbox/config" XDG_CACHE_HOME="$sandbox/cache" XDG_DATA_HOME="$sandbox/data" \
        XDG_CONFIG_DIRS="$sandbox/config" XDG_RUNTIME_DIR="$sandbox/runtime" WAYLAND_DISPLAY="$host_display" \
        PIPEWIRE_RUNTIME_DIR="$host_runtime" GTK_A11Y=none NO_AT_BRIDGE=1 \
        SERVICE={{ root }}/target/release/quotaline-gnome \
        dbus-run-session -- sh -euc '
            gsettings set org.gnome.shell disable-user-extensions false
            gsettings set org.gnome.shell enabled-extensions "[\"{{ uuid }}\"]"
            "$SERVICE" --demo > "$HOME/../service.log" 2>&1 &
            gnome-shell --devkit --wayland --no-x11 > "$HOME/../shell.log" 2>&1
        '

# Remove installed binaries and extension while keeping settings and history
uninstall:
    gjs -c 'const Gio = imports.gi.Gio; const settings = new Gio.Settings({schema_id: "org.gnome.shell"}); settings.set_strv("enabled-extensions", settings.get_strv("enabled-extensions").filter(item => item !== "{{ uuid }}"))'
    gnome-extensions uninstall {{ uuid }} >/dev/null 2>&1 || true
    rm -rf {{ home }}/.local/share/gnome-shell/extensions/{{ uuid }}
    systemctl --user disable --now quotaline-gnome.service || true
    rm -f {{ home }}/.config/systemd/user/quotaline-gnome.service {{ home }}/.local/share/dbus-1/services/tech.bernat.Quotaline.Gnome.service {{ home }}/.local/libexec/quotaline-gnome
    systemctl --user daemon-reload

# Stream local service logs
logs:
    journalctl --user -u quotaline-gnome -f

# Run the self-tests of the CI and packaging scripts
[group('macos')]
[working-directory('macos')]
macos-test-scripts: macos-test-python
    mkdir -p {{ tmp }}
    for test in ci-change-base commit-verdict desktop-guard render-capture ui-test-inventory; do TMPDIR={{ tmp }} bash "Scripts/test-$test.sh"; done

# Build the package
[group('macos')]
[working-directory('macos')]
macos-build: macos-toolchain macos-bindings
    swift build --jobs "${QUOTALINE_BUILD_JOBS:-2}"

# Check the toolchain can build this app, and say what to fix when it cannot
[group('macos')]
[working-directory('macos')]
macos-toolchain:
    @Scripts/check-toolchain.sh

# Run nonpresenting tests without user data or desktop access
[group('macos')]
[working-directory('macos')]
macos-test *filter: macos-bindings
    just macos-test-prepared {{ if filter == "" { "" } else { quote(filter) } }}

[group('macos')]
[working-directory('macos')]
macos-test-prepared *filter: _tmp
    Scripts/check-test-isolation.sh
    Scripts/with-render-checks.sh {{ if filter == "" { "16" } else { "0" } }} ${QUOTALINE_RENDER_CAPTURE_ONLY:+--capture-only} env -u QUOTALINE_TEST_DESKTOP -u QUOTALINE_COMPILE_NATIVE_TESTS swift test --jobs "${QUOTALINE_BUILD_JOBS:-2}" --no-parallel {{ if filter == "" { "" } else { "--filter " + quote(filter) } }}

# Compile native package tests without launching them
[group('macos')]
[working-directory('macos')]
macos-compile-native-tests: macos-bindings
    env -u QUOTALINE_TEST_DESKTOP QUOTALINE_COMPILE_NATIVE_TESTS=1 swift build --build-tests --jobs "${QUOTALINE_BUILD_JOBS:-2}" --scratch-path .build/native-compile

# Run all package tests on an isolated GitHub-hosted desktop
[group('macos')]
[working-directory('macos')]
macos-test-native: _tmp
    just _macos-with-test-desktop just macos-bindings macos-test-native-prepared

[group('macos')]
[working-directory('macos')]
macos-test-native-prepared: _tmp
    #!/usr/bin/env bash
    set -euo pipefail
    source Scripts/check-test-desktop.sh
    Scripts/check-test-isolation.sh
    test_status=0
    Scripts/with-render-checks.sh 16 ${QUOTALINE_RENDER_CAPTURE_ONLY:+--capture-only} swift test --jobs "${QUOTALINE_BUILD_JOBS:-2}" --no-parallel || test_status=$?
    Scripts/check-cached-test-guard.sh
    exit "$test_status"

# Run every test and enforce full coverage on an isolated GitHub-hosted desktop
[group('macos')]
[working-directory('macos')]
macos-coverage: _tmp
    just _macos-with-test-desktop just macos-bindings macos-coverage-prepared

[group('macos')]
[positional-arguments]
[working-directory('macos')]
macos-coverage-prepared scratch=".build/coverage" *args: _tmp
    #!/usr/bin/env bash
    set -euo pipefail
    source Scripts/check-test-desktop.sh
    Scripts/check-test-isolation.sh
    scratch="$1"
    shift
    bin_dir="$(swift build --scratch-path "$scratch" --show-bin-path)"
    rm -rf "$bin_dir/codecov"
    status=0
    Scripts/with-render-checks.sh 16 ${QUOTALINE_RENDER_CAPTURE_ONLY:+--capture-only} swift test --scratch-path "$scratch" --jobs "${QUOTALINE_BUILD_JOBS:-2}" --enable-code-coverage --no-parallel -Xswiftc -Xllvm -Xswiftc -instrprof-atomic-counter-update-all "$@" || status=$?
    uv run Scripts/swift_coverage.py "$bin_dir"
    Scripts/check-cached-test-guard.sh --scratch-path "$scratch"
    exit "$status"

[group('macos')]
[working-directory('macos')]
macos-coverage-rust-native:
    test $(wc -l < adapter/src/credentials/keychain_callback.rs) -le 8
    test $(wc -l < adapter/src/credentials/source_chain_callback.rs) -le 8
    test $(wc -l < adapter/src/http/native_callback.rs) -le 10
    test $(wc -l < adapter/src/time/clock_callback.rs) -le 10
    test $(wc -l < adapter/tests/support/keychain.rs) -le 9
    test $(wc -l < adapter/tests/support/http_transport.rs) -le 10
    test $(wc -l < platform/tests/support/keychain.rs) -le 9
    mkdir -p {{ tmp }}
    TMPDIR={{ tmp }} RUSTC_BOOTSTRAP=1 cargo llvm-cov --locked -p quotaline-macos-platform -p quotaline-macos-ffi --branch --json --ignore-filename-regex '/macos/adapter/src/(credentials/(keychain|source_chain)|http/native|time/clock)_callback[.]rs|/macos/(adapter|platform)/tests/support/keychain[.]rs|/support/http_transport[.]rs' --output-path ../target/macos-rust-coverage.json
    jq -e '[.data[0].files[] | select(.filename | test("/macos/(platform|adapter)/src/"))] | length > 0 and all(.[]; .summary.lines.count > 0 and .summary.lines.covered == .summary.lines.count and .summary.branches.covered == .summary.branches.count)' ../target/macos-rust-coverage.json

# Stream the app's unified log
[group('macos')]
[working-directory('macos')]
macos-logs:
    log stream --level info --style compact --predicate 'subsystem == "tech.bernat.quotaline"'

# Format Swift in place
[group('macos')]
[working-directory('macos')]
macos-fmt:
    swift format -i -r src tests

# Run every repository hook
[group('macos')]
[working-directory('macos')]
macos-lint:
    just pre-commit

[group('macos')]
[working-directory('macos')]
macos-lint-swift:
    swift format lint --strict --recursive src/Quotaline src/QuotalineApp src/QuotalineCore src/QuotalineUI src/QuotalineWidgetApp src/QuotalineWidgets tests App adapter/tests Package.swift
    swiftlint lint --strict --quiet

# Build, run nonpresenting tests, and lint; CI also requires native tests and coverage
[group('macos')]
[working-directory('macos')]
macos-check: macos-toolchain macos-build macos-test macos-lint

# Assemble an ad-hoc signed .app in dist/ for machines without Xcode
[group('macos')]
[working-directory('macos')]
macos-app *args: macos-toolchain macos-bindings
    Scripts/bundle-dev.sh {{ args }}

# Assemble the .app and launch it with real provider data
[group('macos')]
[working-directory('macos')]
macos-run: macos-bindings
    Scripts/bundle-dev.sh --run

# Assemble the .app and launch it with isolated demo data
[group('macos')]
[working-directory('macos')]
macos-run-demo: macos-bindings
    Scripts/bundle-dev.sh --run-demo

# Build the app and install it into /Applications
[group('macos')]
[working-directory('macos')]
_macos-install: macos-bindings
    #!/usr/bin/env bash
    set -euo pipefail
    installed="/Applications/Quotaline.app"
    if [[ -d "$installed" && "${QUOTALINE_FORCE_INSTALL:-0}" != "1" ]]; then
      authority="$(codesign -dv --verbose=4 "$installed" 2>&1 | sed -n 's/^Authority=//p' | head -1 || true)"
      if [[ -n "$authority" && "$authority" != "Apple Development: "* ]]; then
        echo "Refusing to replace a distribution-signed app. Set QUOTALINE_FORCE_INSTALL=1 to replace it." >&2
        exit 1
      fi
    fi
    Scripts/bundle-dev.sh
    if [[ -d "$installed" ]]; then
      osascript -e 'quit app "Quotaline"' 2>/dev/null || true
      for _ in {1..20}; do
        pgrep -x Quotaline >/dev/null || break
        sleep 0.25
      done
      if pgrep -x Quotaline >/dev/null; then
        echo "Quotaline is still running; installation stopped." >&2
        exit 1
      fi
      backup="$(mktemp -d /Applications/.quotaline-install.XXXXXX)"
      mv "$installed" "$backup/"
      installed_new_app=false
      trap 'if [[ "$installed_new_app" != "true" ]]; then rm -rf "$installed"; mv "$backup/Quotaline.app" "$installed"; fi; rm -rf "$backup"' EXIT
    fi
    ditto "dist/Quotaline.app" "$installed"
    if [[ -n "${backup:-}" ]]; then
      installed_new_app=true
      rm -rf "$backup"
      trap - EXIT
    fi

# Install the app and launch it with real provider data
[group('macos')]
[working-directory('macos')]
macos-install: _macos-install
    open -a "Quotaline"

# Install the app and launch it with isolated demo data
[group('macos')]
[working-directory('macos')]
macos-install-demo: _macos-install
    open -n -a "Quotaline" --args --verify-ui

# Re-render the website screenshots from demo data
[group('macos')]
[working-directory('macos')]
macos-shots:
    Scripts/screenshots.sh

# Redraw App/Assets.xcassets from the icon the app draws in code
[group('macos')]
[working-directory('macos')]
macos-icons:
    #!/usr/bin/env bash
    set -euo pipefail
    set="App/Assets.xcassets/AppIcon.appiconset"
    rm -rf "$set" && mkdir -p "$set"
    swift run Quotaline --export-icon "$set" >/dev/null
    python3 - "$set" <<'PYTHON'
    import json, pathlib, sys
    icons = pathlib.Path(sys.argv[1])
    images = [
        {"filename": f"icon_{size}x{size}{suffix}.png", "idiom": "mac", "scale": scale, "size": f"{size}x{size}"}
        for size in (16, 32, 128, 256, 512)
        for suffix, scale in (("", "1x"), ("@2x", "2x"))
    ]
    for file in icons.glob("*.png"):
        if file.name not in {image["filename"] for image in images}:
            file.unlink()
    (icons / "Contents.json").write_text(json.dumps({"images": images, "info": {"author": "xcode", "version": 1}},
                                                    indent=2) + "\n")
    PYTHON
    python3 Scripts/optimize-png.py "$set"

# Generate the Xcode project from App/project.yml
[group('macos')]
[working-directory('macos')]
macos-xcode:
    cd App && xcodegen generate

# Run functional application tests on an isolated GitHub-hosted desktop
[group('macos')]
[working-directory('macos')]
macos-ui: macos-bindings macos-xcode
    Scripts/with-ui-artifacts.sh xcodebuild -project App/Quotaline.xcodeproj -scheme Quotaline-Direct -configuration Verification -destination 'platform=macOS' -derivedDataPath .build/ui-derived -clonedSourcePackagesDirPath .build/xcode-packages -only-testing:QuotalineApplicationUITests ARCHS="$(uname -m)" ONLY_ACTIVE_ARCH=YES test

# Run selected mock-app UI tests on this desktop
[group('macos')]
[positional-arguments]
[working-directory('macos')]
macos-ui-local +tests:
    uv run Scripts/local_ui_tests.py "$@"

# Run opt-in benchmarks on the latest deployed macOS
[group('macos')]
[positional-arguments]
[working-directory('macos')]
macos-benchmark +tests='all':
    uv run Scripts/local_ui_tests.py --suite performance "$@"

# Print the version git says this working tree is
[group('macos')]
[working-directory('macos')]
macos-version format="--full":
    @Scripts/version.sh {{ quote(format) }}

# Write a version into App/project.yml, deriving it from git when no tag is given
[group('macos')]
[working-directory('macos')]
macos-stamp tag="":
    #!/usr/bin/env bash
    set -euo pipefail
    tag={{ quote(tag) }}
    version="${tag:-$(just macos-version --marketing)}"
    source_version="${tag:-$(just macos-version --full)}"
    build="$(just macos-version --build)"
    /usr/bin/sed -i '' "s/MARKETING_VERSION: \".*\"/MARKETING_VERSION: \"$version\"/" App/project.yml
    /usr/bin/sed -i '' "s/CURRENT_PROJECT_VERSION: \".*\"/CURRENT_PROJECT_VERSION: \"$build\"/" App/project.yml
    /usr/bin/sed -i '' "s/SOURCE_VERSION: \".*\"/SOURCE_VERSION: \"$source_version\"/" App/project.yml
    echo "stamped $source_version ($build)"

# Start a release: Prepare Release tags the commit and dispatches the build, the cask and the App Store upload
[group('macos')]
[working-directory('macos')]
macos-release bump="auto":
    gh workflow run "Prepare Release" --field bump={{ bump }}
    @echo "Watch it with: gh run watch \$(gh run list --workflow 'Prepare Release' --limit 1 --json databaseId -q '.[0].databaseId')"

# Archive and export the Developer ID build into dist/direct
[group('macos')]
[working-directory('macos')]
macos-build-direct:
    #!/usr/bin/env bash
    set -euo pipefail

    signed="${SIGNED:-false}"
    scheme="${SCHEME:-Quotaline-Direct}"
    configuration="${CONFIGURATION:-Release}"
    out="${OUT_DIR:-dist/direct}"
    expected_distribution="${EXPECTED_DISTRIBUTION:-Direct}"
    expected_updater="${EXPECTED_UPDATER:-required}"
    archive="$out/Quotaline.xcarchive"
    placeholder_key="AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    public_key="${SPARKLE_PUBLIC_ED_KEY:-$placeholder_key}"
    # Only the Direct build embeds the updater, so only it needs the release key; Homebrew and App Store forbid Sparkle.
    if [[ "$signed" == "true" && "$expected_updater" == "required" && "$public_key" == "$placeholder_key" ]]; then
      echo "a signed build needs SPARKLE_PUBLIC_ED_KEY set to the release EdDSA public key" >&2
      exit 1
    fi
    rm -rf "$out"
    mkdir -p "$out"

    if [[ "$signed" == "true" ]]; then
      xcodebuild -project App/Quotaline.xcodeproj -scheme "$scheme" -configuration "$configuration" \
        -destination 'platform=macOS' -archivePath "$archive" \
        DEVELOPMENT_TEAM="${TEAM_ID:?}" SPARKLE_PUBLIC_ED_KEY="$public_key" \
        APP_GROUP_ID="${TEAM_ID}.tech.bernat.quotaline" \
        SELF_UPDATE_ENABLED="$([[ "$expected_updater" == "required" ]] && echo YES || echo NO)" archive | tail -20
      cat > "$out/export.plist" << PLIST
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
      <key>method</key><string>developer-id</string>
      <key>teamID</key><string>${TEAM_ID}</string>
      <key>signingStyle</key><string>manual</string>
      <key>signingCertificate</key><string>Developer ID Application</string>
    </dict>
    </plist>
    PLIST
      xcodebuild -exportArchive -archivePath "$archive" -exportOptionsPlist "$out/export.plist" \
        -exportPath "$out" | tail -20
    else
      # The app group entitlement needs a provisioning profile, which an unsigned build has none of, so signing is off
      # for the archive and the bundle is ad-hoc signed below instead.
      xcodebuild -project App/Quotaline.xcodeproj -scheme "$scheme" -configuration "$configuration" \
        -destination 'platform=macOS' -archivePath "$archive" \
        CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" CODE_SIGN_STYLE=Manual \
        DEVELOPMENT_TEAM="" SPARKLE_PUBLIC_ED_KEY="$public_key" archive | tail -20
      cp -R "$archive/Products/Applications/Quotaline.app" "$out/"
      codesign --force --deep --sign - "$out/Quotaline.app"
    fi

    Scripts/verify-deployment-targets.sh "$out/Quotaline.app" 15.0
    just macos-verify-app-bundle "$out/Quotaline.app" "$expected_distribution" "$expected_updater"
    codesign --verify --deep --strict --verbose=2 "$out/Quotaline.app"
    echo "exported $out/Quotaline.app (scheme=$scheme, signed=$signed)"

# Archive and export the Homebrew build without Sparkle into dist/homebrew
[group('macos')]
[working-directory('macos')]
macos-build-homebrew:
    SCHEME=Quotaline-Homebrew CONFIGURATION=Homebrew OUT_DIR=dist/homebrew EXPECTED_DISTRIBUTION=Homebrew EXPECTED_UPDATER=forbidden just macos-build-direct

# Archive the sandboxed build and upload it to App Store Connect
[group('macos')]
[working-directory('macos')]
macos-build-app-store:
    #!/usr/bin/env bash
    set -euo pipefail

    : "${TEAM_ID:?}"
    : "${APP_STORE_CONNECT_KEY_ID:?}"
    : "${APP_STORE_CONNECT_ISSUER_ID:?}"
    : "${APP_STORE_CONNECT_KEY_BASE64:?}"
    # The profile names have to match the ones installed on the runner and the ones the project archives with.
    APP_PROFILE="${APP_PROFILE:-Quotaline App Store}"
    WIDGET_PROFILE="${WIDGET_PROFILE:-Quotaline Widget App Store}"
    out="dist/app-store"
    archive="$out/Quotaline.xcarchive"
    key="$RUNNER_TEMP/AuthKey_${APP_STORE_CONNECT_KEY_ID}.p8"
    umask 077
    trap 'rm -f "$key"' EXIT
    echo "$APP_STORE_CONNECT_KEY_BASE64" | base64 --decode > "$key"
    rm -rf "$out"
    mkdir -p "$out"

    xcodebuild -project App/Quotaline.xcodeproj -scheme Quotaline-AppStore -configuration AppStore \
      -destination 'platform=macOS' -archivePath "$archive" DEVELOPMENT_TEAM="$TEAM_ID" archive | tail -20

    app="$archive/Products/Applications/Quotaline.app"
    Scripts/verify-deployment-targets.sh "$app" 15.0
    just macos-verify-app-bundle "$app" "App Store" forbidden

    # Xcode matches a certificate name against the common name, and the portal issues installer certificates under a name
    # it no longer shows, so the hash from the keychain is the only spelling that cannot drift.
    installer_sha="$(security find-certificate -a -c '3rd Party Mac Developer Installer' -Z | awk '/SHA-1 hash:/ {print $3; exit}')"
    if [ -z "$installer_sha" ]; then
      echo "::error::The keychain search list holds no 3rd Party Mac Developer Installer certificate for $TEAM_ID"
      security find-identity -v
      exit 1
    fi

    cat > "$out/export.plist" << PLIST
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
      <key>method</key><string>app-store-connect</string>
      <key>destination</key><string>upload</string>
      <key>teamID</key><string>${TEAM_ID}</string>
      <key>signingStyle</key><string>manual</string>
      <key>signingCertificate</key><string>Apple Distribution</string>
      <key>installerSigningCertificate</key><string>${installer_sha}</string>
      <key>provisioningProfiles</key>
      <dict>
        <key>tech.bernat.quotaline</key><string>${APP_PROFILE}</string>
        <key>tech.bernat.quotaline.widget</key><string>${WIDGET_PROFILE}</string>
      </dict>
    </dict>
    </plist>
    PLIST

    xcodebuild -exportArchive -archivePath "$archive" -exportOptionsPlist "$out/export.plist" -exportPath "$out" \
      -allowProvisioningUpdates -authenticationKeyPath "$key" -authenticationKeyID "$APP_STORE_CONNECT_KEY_ID" \
      -authenticationKeyIssuerID "$APP_STORE_CONNECT_ISSUER_ID" | tail -30
    rm -f "$key"

# Zip, package and checksum the Developer ID build
[group('macos')]
[working-directory('macos')]
macos-package tag:
    Scripts/package-direct.sh {{ tag }}

# Zip, package and checksum the Homebrew build
[group('macos')]
[working-directory('macos')]
macos-package-homebrew tag:
    OUT_DIR=dist/homebrew ARCHIVE_BASENAME=Quotaline-Homebrew EXPECTED_DISTRIBUTION=Homebrew EXPECTED_UPDATER=forbidden just macos-package {{ quote(tag) }}

# Notarize and staple everything in a directory
[group('macos')]
[working-directory('macos')]
macos-notarize directory="dist/direct":
    Scripts/notarize.sh {{ directory }}

# Import a base64 signing certificate into a temporary keychain (CI)
[group('macos')]
[working-directory('macos')]
macos-import-certificate:
    Scripts/import-certificate.sh

# Write the Sparkle appcast for a release
[group('macos')]
[working-directory('macos')]
macos-appcast tag:
    Scripts/appcast.sh {{ tag }}

# Point the Homebrew cask at a released DMG
[group('macos')]
[working-directory('macos')]
macos-cask tag dmg="dist/homebrew/Quotaline-Homebrew.dmg":
    Scripts/update-cask.sh {{ tag }} {{ dmg }}

[group('macos')]
[working-directory('macos')]
macos-test-python:
    mkdir -p {{ tmp }}
    uv venv --allow-existing --python 3.15 .build/verification-tools
    uv pip install --python .build/verification-tools/bin/python \
      psutil==7.2.2 types-psutil==7.2.2.20260906 pytest==9.1.1 pytest-mock==3.16.0 pytest-cov==7.1.0 ty==0.0.88 PyYAML types-PyYAML
    .build/verification-tools/bin/ty check --python .build/verification-tools/bin/python \
      Scripts/local_ui_tests.py Scripts/swift_coverage.py Scripts/verify_app_bundle.py Scripts/with_hang_samples.py Scripts/tests
    .build/verification-tools/bin/python -m pytest Scripts/tests -q --cov=Scripts --cov-report=
    .build/verification-tools/bin/python -m coverage report \
      --include='Scripts/local_ui_tests.py,Scripts/swift_coverage.py,Scripts/verify_app_bundle.py,Scripts/with_hang_samples.py,Scripts/tests/*' --fail-under=100 --show-missing

[positional-arguments]
[private]
[working-directory('macos')]
_macos-with-test-desktop +command: _tmp
    #!/usr/bin/env bash
    set -euo pipefail
    source Scripts/check-test-desktop.sh
    exec "$@"

[group('macos')]
[working-directory('macos')]
macos-verify-app-bundle app distribution updater:
    uv run Scripts/verify_app_bundle.py {{ quote(app) }} {{ quote(distribution) }} {{ quote(updater) }}

[group('macos')]
[positional-arguments]
[working-directory('macos')]
macos-with-hang-samples minutes directory +command: _tmp
    uv run Scripts/with_hang_samples.py "$@"
