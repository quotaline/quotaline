import AppKit
import QuotalineCore
import SwiftUI

@MainActor
public enum ExportRunner {
  public static func execute(
    _ invocation: ExportInvocation, output: (String) -> Void, errorOutput: (String) -> Void
  ) async -> Int32 {
    do {
      output(try await run(invocation))
      return 0
    } catch {
      errorOutput("\(invocation.failureMessage): \(error)\n")
      return 1
    }
  }

  public static func run(
    _ invocation: ExportInvocation, supportDirectory: URL = LiveDependencies.Paths().supportDirectory
  ) async throws -> String {
    switch invocation {
    case .files(let command, let directory):
      // Fix the preview clock so exports keep representative usage levels across reruns.
      let written = try await run(command, directory: directory, now: Date(timeIntervalSince1970: 1_792_215_000))
      return "wrote \(written.count) files to \(directory.path)"
    case .usageJSON:
      return try UsageExportCommand.output(
        cache: SnapshotCache(url: supportDirectory.appendingPathComponent("snapshots.json")))
    }
  }

  public static func run(
    _ command: ExportCommand, directory: URL, now: Date = Date(), settle: Duration = .seconds(2),
    temporaryDirectory: URL = FileManager.default.temporaryDirectory,
    suite: @escaping @Sendable (String) -> UserDefaults = persistentDefaults(suiteName:)
  ) async throws -> [URL] {
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    switch command {
    case .icons: return try exportIcons(to: directory)
    case .menuBar: return try exportMenuBar(to: directory, now: now)
    case .popover:
      let policy = LaunchPolicy(
        arguments: [LaunchPolicy.verificationArgument], environment: [:],
        temporaryDirectory: temporaryDirectory, verificationIdentifier: UUID().uuidString, suite: suite)
      let result: Result<[URL], any Error>
      do {
        result = .success(try await exportPopover(to: directory, settle: settle, policy: policy))
      } catch {
        result = .failure(error)
      }
      try policy.cleanup()
      return try result.get()
    }
  }

  private static func exportIcons(to directory: URL) throws -> [URL] {
    try AppIcon.exportIconSet(to: directory)
    return try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil).sorted {
      $0.path < $1.path
    }
  }

  private static func exportMenuBar(to directory: URL, now: Date) throws -> [URL] {
    let snapshots = Dictionary(
      uniqueKeysWithValues: [ProviderID.claude, .codex].map { ($0, DemoData.snapshot($0, now: now)) })
    let model = StatusItemBuilder.build(
      StatusItemInput(
        snapshots: snapshots, availability: snapshots.mapValues { _ in .current },
        selectedKeys: StatusItemBuilder.defaultSelection(snapshots), format: .stacked, customTemplate: "", decimals: 0,
        hideZeroCells: true, order: .provider, labels: [:], now: now))
    var written: [URL] = []
    for (name, dark) in [("menubar-light", false), ("menubar-dark", true)] {
      let data = StatusItemRenderer.stripData(for: model, dark: dark)
      let url = directory.appendingPathComponent("\(name).png")
      try data.write(to: url)
      written.append(url)
    }
    return written
  }

  private static func exportPopover(to directory: URL, settle: Duration, policy: LaunchPolicy) async throws -> [URL] {
    let delegate = try await AppRunner.bootstrap(
      distribution: .direct, notificationCenter: nil, updater: nil, isSandboxed: false,
      paths: LiveDependencies.Paths(
        supportDirectory: policy.supportDirectory!, environment: policy.environment,
        arguments: [LaunchPolicy.verificationArgument], verificationProfile: policy.verificationProfile),
      defaults: policy.defaults(), transport: DisabledHTTPTransport(), keychain: .empty, launchAtLogin: .inMemory())
    let controller = delegate.controller
    await controller.dependencies.demoSeedTask?.value
    let result: Result<[URL], any Error>
    do {
      result = .success(try await renderPopover(controller, to: directory, settle: settle))
    } catch {
      result = .failure(error)
    }
    controller.stop()
    await controller.flushPersistence()
    return try result.get()
  }

  private static func renderPopover(
    _ controller: AppController, to directory: URL, settle: Duration
  ) async throws -> [URL] {
    // A month of daily buckets shows the weekly rhythm; the default "today" view has nothing to draw yet
    controller.environment.settings.historyRange = .month
    controller.environment.settings.historyRollup = .day
    await controller.coordinator.refresh(RefreshRequest(reason: .export, usage: .force, analytics: .force))
    controller.environment.historyPresenter.setActive(true)
    controller.environment.historyPresenter.setDataScope(
      HistoryDataScope(
        activeProviders: controller.environment.settings.activeProviders(
          states: controller.environment.state.providers),
        selectedWindows: Set(controller.environment.settings.modelSelection(in: controller.environment.state.snapshots))
      ))
    controller.environment.historyPresenter.reload()
    await controller.environment.historyPresenter.waitForLoad()
    try? await Task.sleep(for: settle)
    await controller.environment.loadRecentSamples(force: true)
    await controller.environment.spendSummary.load(
      history: controller.environment.history,
      providers: controller.environment.settings.activeProviders(states: controller.environment.state.providers),
      now: controller.environment.clock.now(), timeZone: .current)
    var written: [URL] = []
    for tab in PopoverTab.allCases {
      controller.environment.settings.lastTab = tab
      for (suffix, dark) in [("light", false), ("dark", true)] {
        let measured = MeasuredSize()
        let view = RootView(
          environment: controller.environment,
          onMeasure: { measurement in
            if measurement.tab == tab { measured.value = measurement.size }
          })
        // Measurement arrives on the next run-loop turn after the viewport has laid out its content.
        let hosting = view.makeNativeView()
        hosting.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        hosting.frame = CGRect(origin: .zero, size: shotSize)
        hosting.layoutSubtreeIfNeeded()
        await controller.environment.historyPresenter.waitForLoad()
        try? await Task.sleep(for: .milliseconds(50))
        let size = exportSize(measured: measured.value, fallback: shotSize)
        let data = PopoverExporter.png(hosting, dark: dark, size: size)!
        let url = directory.appendingPathComponent("popover-\(tab.rawValue.lowercased())-\(suffix).png")
        try data.write(to: url)
        written.append(url)
      }
    }
    return written
  }

  /// A laptop-sized viewport for measuring the tab's full content.
  static var shotSize: CGSize {
    let screen = CGRect(x: 0, y: 0, width: 1512, height: 944)
    let anchor = CGRect(x: screen.midX, y: screen.maxY - 24, width: 40, height: 24)
    return CGSize(
      width: PopoverGeometry.stableWidth(),
      height: min(PopoverGeometry.maxSize(anchor: anchor, visibleFrame: screen).height, 760))
  }

  static func exportSize(measured: CGSize, fallback: CGSize) -> CGSize {
    CGSize(
      width: fallback.width,
      height: measured.height > 0 ? measured.height : fallback.height)
  }
}

@MainActor
final class MeasuredSize {
  var value: CGSize = .zero
}
