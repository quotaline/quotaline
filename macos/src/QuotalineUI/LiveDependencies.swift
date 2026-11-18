import AppKit
import QuotalineCore
import UniformTypeIdentifiers
import WidgetKit

public enum LiveDependencies {
  public typealias WorkspaceOpen =
    @MainActor @Sendable (
      URL, NSWorkspace.OpenConfiguration, @escaping @Sendable (Result<Void, any Error>) -> Void
    ) -> Void

  public struct Paths: Sendable {
    public var home: URL
    public var supportDirectory: URL
    public var environment: [String: String]
    public var userName: String
    public var arguments: [String]
    public var verificationProfile: VerificationProfile?

    public init(
      home: URL = FileManager.default.homeDirectoryForCurrentUser,
      supportDirectory: URL = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("Quotaline"),
      environment: [String: String] = ProcessInfo.processInfo.environment,
      userName: String = NSUserName(),
      arguments: [String] = CommandLine.arguments,
      verificationProfile: VerificationProfile? = nil
    ) {
      self.home = home
      self.supportDirectory = supportDirectory
      self.environment = environment
      self.userName = userName
      self.arguments = arguments
      self.verificationProfile = verificationProfile
    }

    public var demoRequested: Bool {
      environment["QUOTALINE_DEMO"] != nil || arguments.contains("--demo")
    }
  }

  @MainActor
  public static func make(
    appInfo: AppInfo,
    paths: Paths = Paths(),
    defaults: UserDefaults = .standard,
    notificationCenter: (any NotificationCenterProtocol)?,
    updater: (any UpdaterHook)? = nil,
    isSandboxed: Bool = false,
    transport: any HTTPTransport,
    keychain: KeychainCredentialClient,
    launchAtLogin: LaunchAtLoginBackend,
    workspaceOpen: WorkspaceOpen? = nil
  ) async throws -> AppDependencies {
    let log = LogBuffer(fileURL: paths.supportDirectory.appendingPathComponent("log.txt"))
    let settings = QuotalineCore.Settings(defaults: defaults)
    let isDemo = settings.demoMode ?? paths.demoRequested
    let history = try UsageHistoryStore(
      url: paths.supportDirectory.appendingPathComponent(isDemo ? "usage-demo.sqlite" : "usage.sqlite"),
      retentionDays: settings.historyRetentionDays)
    let resolvedWorkspaceOpen = resolvedWorkspaceOpen(workspaceOpen)
    return await assemble(
      appInfo: appInfo, paths: paths, notificationCenter: notificationCenter, updater: updater,
      isSandboxed: isSandboxed, transport: transport, keychain: keychain, log: log, settings: settings,
      history: history, isDemo: isDemo, launchAtLogin: launchAtLogin, workspaceOpen: resolvedWorkspaceOpen)
  }

  @MainActor
  public static func makeDeferred(
    appInfo: AppInfo,
    paths: Paths = Paths(),
    defaults: UserDefaults = .standard,
    notificationCenter: (any NotificationCenterProtocol)?,
    updater: (any UpdaterHook)? = nil,
    isSandboxed: Bool = false,
    transport: any HTTPTransport,
    keychain: KeychainCredentialClient,
    launchAtLogin: LaunchAtLoginBackend,
    workspaceOpen: WorkspaceOpen? = nil,
    legacyPreferences: (@Sendable (String) -> [String: Any]?)? = nil
  ) async throws -> AppDependencies {
    let legacyImport = try legacyPreferences.flatMap {
      try LegacyImport.run(
        defaults: defaults, domain: appInfo.bundleIdentifier, persistentDomain: $0,
        supportDirectory: paths.supportDirectory)
    }
    let settings = QuotalineCore.Settings(defaults: defaults)
    let isDemo = settings.demoMode ?? paths.demoRequested
    let supportDirectory = paths.supportDirectory
    let retentionDays = settings.historyRetentionDays
    let logTask = Task.detached(priority: .utility) {
      LogBuffer(fileURL: supportDirectory.appendingPathComponent("log.txt"))
    }
    let historyTask = Task.detached(priority: .utility) {
      try UsageHistoryStore(
        url: supportDirectory.appendingPathComponent(isDemo ? "usage-demo.sqlite" : "usage.sqlite"),
        retentionDays: retentionDays)
    }
    let log = await logTask.value
    if let legacyImport { log.logInfo(legacyImport) }
    let history = try await historyTask.value
    let resolvedWorkspaceOpen = resolvedWorkspaceOpen(workspaceOpen)
    return await assemble(
      appInfo: appInfo, paths: paths, notificationCenter: notificationCenter, updater: updater,
      isSandboxed: isSandboxed, transport: transport, keychain: keychain, log: log, settings: settings,
      history: history, isDemo: isDemo, launchAtLogin: launchAtLogin, workspaceOpen: resolvedWorkspaceOpen)
  }

  @MainActor
  private static func assemble(
    appInfo: AppInfo,
    paths: Paths,
    notificationCenter: (any NotificationCenterProtocol)?,
    updater: (any UpdaterHook)?,
    isSandboxed: Bool,
    transport: any HTTPTransport,
    keychain: KeychainCredentialClient,
    log: LogBuffer,
    settings: QuotalineCore.Settings,
    history: UsageHistoryStore,
    isDemo: Bool,
    launchAtLogin: LaunchAtLoginBackend,
    workspaceOpen: @escaping WorkspaceOpen
  ) async -> AppDependencies {
    let verificationProfile = paths.verificationProfile
    let clock = verificationProfile?.clock() ?? .system
    let client = APIClient(transport: transport, log: log)
    let build: @MainActor @Sendable (QuotalineCore.Settings) async -> ProviderRegistry = { settings in
      if isDemo { return demoRegistry(fixture: verificationProfile?.fixture ?? .standard) }
      if verificationProfile != nil { return ProviderRegistry([]) }
      return await providers(
        paths: paths, client: client, log: log, settings: settings, isSandboxed: isSandboxed, keychain: keychain)
    }
    let registry = await build(settings)
    let state = AppState()
    state.applySetupStates(registry.setupStates)
    let demoSeedTask =
      isDemo
      ? seedDemo(history, log: log, now: clock.now(), fixture: verificationProfile?.fixture ?? .standard, state: state)
      : nil
    let effectiveSandboxed = isSandboxed || verificationProfile?.fixture == .controlAudit
    let runtimeActions = runtimeActions(verification: verificationProfile != nil)
    let relaunchAction: @MainActor @Sendable () -> Void = {
      Self.relaunch(
        bundle: .main, policy: LaunchPolicy(arguments: paths.arguments, environment: paths.environment),
        detailedLogging: settings.detailedLogging, settings: settings, open: workspaceOpen,
        onFailure: { error in
          settings.demoMode = isDemo
          log.logError("Could not relaunch: \(error.localizedDescription). Quit and reopen to try again.")
        }
      ) {
        log.logDebug("replacement launch completed; terminating the previous instance")
        NSApplication.shared.terminate(nil)
      }
    }
    return AppDependencies(
      appInfo: appInfo,
      settings: settings,
      state: state,
      history: history,
      log: log,
      registry: registry,
      notifier: Notifier(center: notificationCenter, log: log),
      launchAtLogin: launchAtLogin,
      clock: clock,
      // A refresh re-lays out the panel, so UI tests refresh only on request and never mid-click.
      refreshesAutomatically: verificationProfile == nil,
      updater: updater,
      isSandboxed: effectiveSandboxed,
      isDemo: isDemo,
      demoSeedTask: demoSeedTask,
      openURL: runtimeActions.openURL,
      copyToPasteboard: runtimeActions.copy,
      revealInFinder: runtimeActions.reveal,
      chooseExportURL: exportChooser(profile: verificationProfile, supportDirectory: paths.supportDirectory),
      chooseDirectory: directoryChooser(
        profile: verificationProfile, paths: paths, supportDirectory: paths.supportDirectory),
      terminate: runtimeActions.terminate,
      relaunch: relaunchAction,
      widgetStore: isDemo || verificationProfile != nil ? nil : widgetStore(supportDirectory: paths.supportDirectory),
      snapshotCache: SnapshotCache(
        url: paths.supportDirectory.appendingPathComponent(isDemo ? "snapshots-demo.json" : "snapshots.json")),
      reloadWidgets: { WidgetCenter.shared.reloadAllTimelines() },
      rebuildProviders: build,
      screenVisibleFrame: {
        PopoverGeometry.visibleFrame(
          NSScreen.main?.visibleFrame, cappedTo: verificationProfile?.visibleFrameWidth.map { CGFloat($0) })
      },
      openPopoverOnLaunch: paths.environment["QUOTALINE_OPEN_POPOVER"] != nil,
      persistsStatusItemPosition: verificationProfile == nil,
      recoversOffscreenPopover: verificationProfile != nil,
      verificationSession: paths.environment[LaunchPolicy.verificationSessionKey],
      verificationSnapshotURL: verificationProfile.map { _ in
        paths.supportDirectory.appendingPathComponent("process-snapshot.json")
      },
      openProviderLogin: ProviderLoginLauncher.make(
        directory: paths.supportDirectory.appendingPathComponent("Sign-in"),
        enabled: !isDemo && verificationProfile == nil && !isSandboxed,
        configurationDirectories: ProviderLoginCommand.configurationDirectories(environment: paths.environment),
        open: runtimeActions.openLogin)
    )
  }

  private static func demoRegistry(fixture: VerificationProfile.Fixture) -> ProviderRegistry {
    let providers = ProviderID.allCases.map { DemoProvider(id: $0, fixture: fixture) }
    let setup = Dictionary(
      uniqueKeysWithValues: providers.map { provider in
        return (
          provider.id,
          ProviderSetupState(
            enabled: true, credential: .valid(source: provider.credentialSource, expiresAt: nil),
            resources: fixture == .controlAudit
              ? provider.id.sandboxResources.map { ResourceAccessState(resource: $0, health: .needed) } : [])
        )
      })
    return ProviderRegistry(providers, setupStates: setup)
  }

  @MainActor
  static func exportChooser(
    profile: VerificationProfile?, supportDirectory: URL,
    run: @escaping (NSSavePanel, @escaping (NSApplication.ModalResponse) -> Void) -> Void = {
      presentFilePanel($0, completion: $1)
    }
  ) -> () async -> URL? {
    guard let profile else { return { await chosen({ exportPanel() }, run: run) } }
    guard profile.nativePanels else {
      let url = supportDirectory.appendingPathComponent("verification-history.csv")
      return { url }
    }
    return { await chosen({ exportPanel(default: supportDirectory) }, run: run) }
  }

  @MainActor
  static func directoryChooser(
    profile: VerificationProfile?, paths: Paths, supportDirectory: URL,
    run: @escaping (NSOpenPanel, @escaping (NSApplication.ModalResponse) -> Void) -> Void = {
      presentFilePanel($0, completion: $1)
    }
  ) -> (SandboxResource) async -> URL? {
    guard let profile else {
      return { resource in await chosen({ directoryPanel(resource, paths: paths) }, run: run) }
    }
    guard profile.nativePanels else { return { _ in nil } }
    return { resource in
      await chosen({ directoryPanel(resource: resource, default: supportDirectory) }, run: run)
    }
  }

  public static func widgetStore(
    supportDirectory: URL,
    containerURL: (String) -> URL? = { FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: $0) },
    appGroup: String = WidgetSnapshot.appGroup(info: Bundle.main.infoDictionary)
  ) -> WidgetSnapshotStore {
    WidgetSnapshotStore(
      url: WidgetSnapshotStore.sharedURL(
        containerURL: containerURL, fallbackDirectory: supportDirectory, appGroup: appGroup))
  }

  @discardableResult
  public static func seedDemo(
    _ history: UsageHistoryStore, log: LogBuffer, now: Date = Date(),
    fixture: VerificationProfile.Fixture = .standard, state: AppState? = nil
  ) -> Task<Void, Never> {
    Task {
      do {
        guard try await history.stats().sampleCount == 0 else { return }
        try await DemoData.seed(history, providers: ProviderID.allCases, now: now, fixture: fixture)
        await state?.markSamplesChanged()
        log.log("demo history seeded")
      } catch {
        log.logError("demo history seeding failed: \(error)")
      }
    }
  }

  @MainActor
  public static func relaunch(
    bundle: Bundle,
    policy: LaunchPolicy = LaunchPolicy(),
    detailedLogging: Bool = false,
    settings: QuotalineCore.Settings,
    processIdentifier: pid_t = ProcessInfo.processInfo.processIdentifier,
    open: WorkspaceOpen,
    onFailure: @escaping @MainActor @Sendable (any Error) -> Void,
    then terminate: @escaping @MainActor @Sendable () -> Void
  ) {
    let configuration = NSWorkspace.OpenConfiguration()
    configuration.createsNewApplicationInstance = true
    // A sandboxed caller's arguments never arrive, so the replacement also learns it is one from the handshake.
    configuration.arguments = policy.relaunchArguments(detailedLogging: detailedLogging)
    configuration.environment = policy.relaunchEnvironment
    settings.requestRelaunch(from: processIdentifier)
    open(bundle.bundleURL, configuration) { result in
      // AppKit's termination loop must leave the main dispatch queue free for asynchronous persistence.
      RunLoop.main.perform(inModes: [.common]) {
        MainActor.assumeIsolated {
          switch result {
          case .success: terminate()
          case .failure(let error):
            settings.withdrawRelaunch()
            onFailure(error)
          }
        }
      }
    }
  }

  /// Resolves a home directory the sandbox would otherwise block, using the bookmark the user granted for it.
  @MainActor
  public static func directory(
    _ resource: SandboxResource, paths: Paths, settings: QuotalineCore.Settings, isSandboxed: Bool, log: LogBuffer
  ) -> URL {
    let configured = resource.configuredURL(environment: paths.environment, home: paths.home)
    guard isSandboxed else { return configured }
    return resolve(bookmark: settings.bookmark(for: resource), fallback: configured, log: log)
  }

  public static func resolve(bookmark: Data?, fallback: URL, log: LogBuffer) -> URL {
    guard let bookmark else { return fallback }
    var stale = false
    guard
      let url = try? URL(
        resolvingBookmarkData: bookmark, options: .withSecurityScope, relativeTo: nil, bookmarkDataIsStale: &stale)
    else {
      log.logError("bookmark could not be resolved; falling back to \(fallback.path)")
      return fallback
    }
    if stale { log.log("bookmark for \(url.lastPathComponent) is stale; grant access again if reads fail") }
    return url
  }

  public static func copy(_ text: String, to pasteboard: NSPasteboard) {
    pasteboard.clearContents()
    pasteboard.setString(text, forType: .string)
  }

  @MainActor
  public static func exportPanel(default directory: URL? = nil) -> NSSavePanel {
    let panel = NSSavePanel()
    panel.allowedContentTypes = [.commaSeparatedText]
    panel.nameFieldStringValue = "quotaline-history.csv"
    panel.directoryURL = directory
    return panel
  }

  @MainActor
  public static func directoryPanel(_ resource: SandboxResource, paths: Paths) -> NSOpenPanel {
    directoryPanel(
      resource: resource, default: resource.configuredURL(environment: paths.environment, home: paths.home))
  }

  @MainActor
  public static func directoryPanel(resource: SandboxResource, default directory: URL) -> NSOpenPanel {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.showsHiddenFiles = true
    panel.directoryURL = directory
    panel.message = "Select \(resource.label) so \(resource.provider.displayName) usage can be read."
    return panel
  }

  @MainActor
  public static func chosen<Panel: NSSavePanel>(
    _ makePanel: @escaping () -> Panel,
    run: @escaping (Panel, @escaping (NSApplication.ModalResponse) -> Void) -> Void
  ) async -> URL? {
    await withCheckedContinuation { continuation in
      // Panel construction can enter the file-service run loop, so keep its lifecycle off the Swift task stack.
      DispatchQueue.main.async {
        TooltipPresenter.shared.tearDown()
        let panel = makePanel()
        run(panel) { response in
          let url = response == .OK ? panel.url : nil
          panel.orderOut(nil)
          continuation.resume(returning: url)
        }
      }
    }
  }

  @MainActor
  public static func presentFilePanel(
    _ panel: NSSavePanel, parent: NSWindow? = NSApp.keyWindow,
    completion: @escaping (NSApplication.ModalResponse) -> Void
  ) {
    if let parent {
      panel.beginSheetModal(for: parent, completionHandler: completion)
    } else {
      panel.begin(completionHandler: completion)
    }
  }
}
