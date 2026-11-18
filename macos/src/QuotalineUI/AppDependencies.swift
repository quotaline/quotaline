import AppKit
import QuotalineCore
import SwiftUI

@MainActor
public protocol UpdaterHook: AnyObject {
  var canCheck: Bool { get }
  var automaticallyChecks: Bool { get set }
  var onCanCheckChange: (@MainActor (Bool) -> Void)? { get set }
  func start()
  func checkForUpdates()
}

@MainActor
public struct AppDependencies {
  public var appInfo: AppInfo
  public var settings: QuotalineCore.Settings
  public var state: AppState
  public var history: UsageHistoryStore
  public var log: LogBuffer
  public var registry: ProviderRegistry
  public var notifier: Notifier
  public var launchAtLogin: LaunchAtLoginBackend
  public var clock: Clock
  public var refreshesAutomatically: Bool
  public var updater: (any UpdaterHook)?
  public var statusBar: NSStatusBar
  public var isSandboxed: Bool
  public var isDemo: Bool
  public var demoSeedTask: Task<Void, Never>?
  public var increaseContrast: @MainActor () -> Bool
  public var openURL: @MainActor (URL) -> Void
  public var openProviderLogin: (@MainActor (ProviderLoginCommand) async throws -> Void)?
  public var copyToPasteboard: @MainActor (String) -> Void
  public var revealInFinder: @MainActor (URL) -> Void
  public var chooseExportURL: () async -> URL?
  public var chooseDirectory: (SandboxResource) async -> URL?
  public var terminate: @MainActor () -> Void
  public var relaunch: @MainActor @Sendable () -> Void
  public var widgetStore: WidgetSnapshotStore?
  public var snapshotCache: SnapshotCache
  public var persistence: SnapshotPersistence
  public var reloadWidgets: @MainActor @Sendable () -> Void
  public var rebuildProviders: @MainActor @Sendable (QuotalineCore.Settings) async -> ProviderRegistry
  public var screenVisibleFrame: () -> CGRect?
  public var openPopoverOnLaunch: Bool
  public var presentsWindows: Bool
  public var beginPopoverActivation: PopoverActivationHandler?
  public var persistsStatusItemPosition: Bool
  public var recoversOffscreenPopover: Bool
  public var verificationSession: String?
  public var verificationSnapshotURL: URL?
  public var captureProcessSnapshot: @Sendable () -> ProcessPerformanceSnapshot?

  public init(
    appInfo: AppInfo,
    settings: QuotalineCore.Settings,
    state: AppState,
    history: UsageHistoryStore,
    log: LogBuffer,
    registry: ProviderRegistry,
    notifier: Notifier,
    launchAtLogin: LaunchAtLoginBackend,
    clock: Clock = .system,
    refreshesAutomatically: Bool = true,
    updater: (any UpdaterHook)? = nil,
    statusBar: NSStatusBar = .system,
    isSandboxed: Bool = false,
    isDemo: Bool = false,
    demoSeedTask: Task<Void, Never>? = nil,
    increaseContrast: @escaping @MainActor () -> Bool = {
      NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
    },
    openURL: @escaping @MainActor (URL) -> Void,
    copyToPasteboard: @escaping @MainActor (String) -> Void,
    revealInFinder: @escaping @MainActor (URL) -> Void,
    chooseExportURL: @escaping () async -> URL?,
    chooseDirectory: @escaping (SandboxResource) async -> URL?,
    terminate: @escaping @MainActor () -> Void,
    relaunch: @escaping @MainActor @Sendable () -> Void = {},
    widgetStore: WidgetSnapshotStore? = nil,
    snapshotCache: SnapshotCache = SnapshotCache(url: nil),
    persistence: SnapshotPersistence? = nil,
    reloadWidgets: @escaping @MainActor @Sendable () -> Void = {},
    rebuildProviders: @escaping @MainActor @Sendable (QuotalineCore.Settings) async -> ProviderRegistry,
    screenVisibleFrame: @escaping () -> CGRect?,
    openPopoverOnLaunch: Bool = false,
    presentsWindows: Bool = true,
    beginPopoverActivation: PopoverActivationHandler? = nil,
    persistsStatusItemPosition: Bool = true,
    recoversOffscreenPopover: Bool = false,
    verificationSession: String? = nil,
    verificationSnapshotURL: URL? = nil,
    captureProcessSnapshot: @escaping @Sendable () -> ProcessPerformanceSnapshot? = ProcessPerformanceSnapshot.current,
    openProviderLogin: (@MainActor (ProviderLoginCommand) async throws -> Void)? = nil
  ) {
    self.appInfo = appInfo
    self.settings = settings
    self.state = state
    self.history = history
    self.log = log
    self.registry = registry
    self.notifier = notifier
    self.launchAtLogin = launchAtLogin
    self.clock = clock
    self.refreshesAutomatically = refreshesAutomatically
    self.updater = updater
    self.statusBar = statusBar
    self.isSandboxed = isSandboxed
    self.isDemo = isDemo
    self.demoSeedTask = demoSeedTask
    self.increaseContrast = increaseContrast
    self.openURL = openURL
    self.openProviderLogin = openProviderLogin
    self.copyToPasteboard = copyToPasteboard
    self.revealInFinder = revealInFinder
    self.chooseExportURL = chooseExportURL
    self.chooseDirectory = chooseDirectory
    self.terminate = terminate
    self.relaunch = relaunch
    self.widgetStore = widgetStore
    self.snapshotCache = snapshotCache
    self.persistence =
      persistence
      ?? SnapshotPersistence(
        cache: snapshotCache,
        widgetStore: widgetStore,
        usageFileURL: snapshotCache.url.map {
          $0.deletingLastPathComponent().appendingPathComponent(UsageExport.fileName)
        },
        failureHandler: { failure in log.logError(failure.message) },
        reloadWidgets: reloadWidgets)
    self.reloadWidgets = reloadWidgets
    self.rebuildProviders = rebuildProviders
    self.screenVisibleFrame = screenVisibleFrame
    self.openPopoverOnLaunch = openPopoverOnLaunch
    self.presentsWindows = presentsWindows
    self.beginPopoverActivation = beginPopoverActivation
    self.persistsStatusItemPosition = persistsStatusItemPosition
    self.recoversOffscreenPopover = recoversOffscreenPopover
    self.verificationSession = verificationSession
    self.verificationSnapshotURL = verificationSnapshotURL
    self.captureProcessSnapshot = captureProcessSnapshot
  }
}
