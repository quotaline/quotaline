import AppKit
import QuotalineCore
import SwiftUI

@MainActor
public final class AppController {
  let providerLoginFlow = ProviderLoginFlow()
  public internal(set) var dependencies: AppDependencies
  public let environment: UIEnvironment
  public let coordinator: RefreshCoordinator
  public internal(set) var statusItem: StatusItemController?
  var stopped = false
  var launchPopoverTask: Task<Void, Never>?
  public internal(set) var popover: PopoverController?
  var logWindow: LogWindowController?
  var workspaceObservers: [Any] = []
  var applicationObservers: [Any] = []
  var distributedObservers: [Any] = []
  var verificationSnapshotRequests: VerificationSnapshotRequests?
  var lastPopoverVisibleFrame: CGRect?
  var registry: ProviderRegistry
  var appliedRetentionDays: Int
  var retentionTask: Task<Void, Never>?
  var retentionGeneration = 0
  var widgetSubmissionTask: Task<Void, Never>?
  var providerHealthTask: Task<Void, Never>?
  var providerRediscoveryTask: Task<Void, Never>?
  var lifecycleFlushTask: Task<Void, Never>?
  var providerGeneration = 0
  var providerRediscoveryGeneration = 0
  var providerRediscoveryPolicy = ProviderRediscoveryPolicy()
  var notificationsEnabled: Bool
  var firstRunCardPending = false
  let initialStatusItem: NSStatusItem?
  lazy var menuTarget = MenuTarget(controller: self)

  public init(dependencies: AppDependencies, initialStatusItem: NSStatusItem? = nil) {
    self.dependencies = dependencies
    self.initialStatusItem = initialStatusItem
    registry = dependencies.registry
    appliedRetentionDays = dependencies.settings.historyRetentionDays
    notificationsEnabled = dependencies.settings.notifications.enabled
    let notifier = dependencies.notifier
    let state = dependencies.state
    coordinator = RefreshCoordinator(
      registry: dependencies.registry,
      settings: dependencies.settings,
      state: state,
      history: dependencies.history,
      log: dependencies.log,
      clock: dependencies.clock,
      cache: dependencies.snapshotCache,
      persistence: dependencies.persistence,
      refreshesAutomatically: dependencies.refreshesAutomatically
    ) { [settings = dependencies.settings] events in
      notifier.playSound = settings.notifications.playSound
      Task { await notifier.deliver(events) }
    }
    environment = UIEnvironment(
      state: state,
      settings: dependencies.settings,
      history: dependencies.history,
      log: dependencies.log,
      appInfo: dependencies.appInfo,
      clock: dependencies.clock,
      launchAtLoginStatus: dependencies.launchAtLogin.status(),
      credentialDescriptions: Dictionary(
        uniqueKeysWithValues: dependencies.registry.providers.map { ($0.id, $0.credentialDescription) }),
      canCheckForUpdates: dependencies.updater?.canCheck ?? false,
      isSandboxed: dependencies.isSandboxed,
      isDemo: dependencies.isDemo,
      increaseContrast: dependencies.increaseContrast()
    )
    environment.actions = actions()
    environment.canLaunchProviderLogin =
      dependencies.openProviderLogin != nil && !dependencies.isSandboxed && !dependencies.isDemo
    dependencies.log.debugEnabled = dependencies.settings.detailedLogging
    coordinator.widgetSink = { [weak self] in self?.publishWidget($0) }
  }

  public func publishWidget(_ snapshot: WidgetSnapshot) {
    let previous = widgetSubmissionTask
    widgetSubmissionTask = Task { [persistence = dependencies.persistence] in
      await previous?.value
      await persistence.submitWidget(snapshot)
    }
  }

  public func flushPersistence() async {
    await widgetSubmissionTask?.value
    await coordinator.flushPersistence()
  }

  public func prepareToTerminate() async {
    cancelBackgroundTasks()
    coordinator.stop()
    popover?.close(restoringActivation: false)
    _ = await ShutdownPolicy.waitForCompletion { [weak self] in await self?.flushPersistence() }
  }

  func actions() -> UIActions {
    UIActions(
      refresh: { [weak self] in self?.refreshNow() },
      refreshProvider: { [weak self] in self?.refreshNow(provider: $0) },
      signInProvider: { [weak self] provider in Task { await self?.signInProvider(provider) } },
      showProviders: { [weak self] in self?.showProviders($0) },
      openURL: { [weak self] in self?.dependencies.openURL($0) },
      copy: { [weak self] in self?.dependencies.copyToPasteboard($0) },
      exportHistory: { [weak self] in self?.exportHistory() },
      clearHistory: { [weak self] in self?.clearHistory() },
      revealHistory: { [weak self] in self?.revealHistory() },
      copyDiagnostics: { [weak self] in self?.copyDiagnostics() },
      reportIssue: { [weak self] in self?.reportIssue() },
      showFullLog: { [weak self] in self?.showFullLog() },
      setLaunchAtLogin: { [weak self] in self?.setLaunchAtLogin($0) },
      openLoginItems: { [weak self] in self?.dependencies.launchAtLogin.openSettings() },
      grantAccess: { [weak self] resource in Task { await self?.grantAccess(to: resource) } },
      checkForUpdates: { [weak self] in self?.dependencies.updater?.checkForUpdates() },
      quit: { [weak self] in self?.dependencies.terminate() },
      setDemoMode: { [weak self] in self?.setDemoMode($0) },
      settingsChanged: { [weak self] in self?.settingsChanged() },
      historyRetentionChanged: { [weak self] in self?.applyHistoryRetention() },
      settingsReset: { [weak self] in Task { await self?.settingsReset() } }
    )
  }

  public func start() {
    let log = dependencies.log
    if let previous = dependencies.settings.lastLaunchedVersion, previous != dependencies.appInfo.version {
      log.log("updated from \(previous) to \(dependencies.appInfo.version)")
    }
    firstRunCardPending =
      dependencies.settings.lastLaunchedVersion == nil && dependencies.settings.configuredProviders.isEmpty
      && !dependencies.settings.firstRunCardDismissed
    dependencies.settings.lastLaunchedVersion = dependencies.appInfo.version
    log.log("launch \(dependencies.appInfo.name) \(dependencies.appInfo.version) (\(dependencies.appInfo.build))")
    let item = StatusItemController(
      statusBar: dependencies.statusBar, item: initialStatusItem, log: log,
      autosaveName: dependencies.persistsStatusItemPosition
        ? StatusItemController.autosaveName(bundleIdentifier: Bundle.main.bundleIdentifier) : nil
    ) {
      $0.performClick(nil)
    }
    item.onClick = { [weak self] in self?.togglePopover() }
    item.onCountdownTick = { [weak self] in self?.coordinator.rebuildStatus() }
    item.menuProvider = menuTarget.menu
    item.adaptive = dependencies.settings.adaptiveWidth
    item.detailedLoggingEnabled = dependencies.settings.detailedLogging
    item.update(ladder: dependencies.state.statusLadder)
    statusItem = item
    let popover = PopoverController(
      content: AnyView(EmptyView()), log: log, presentsWindow: dependencies.presentsWindows,
      recoversOffscreenAnchor: dependencies.recoversOffscreenPopover,
      beginActivation: dependencies.beginPopoverActivation)
    popover.setContent(rootView(popover))
    popover.select(tab: dependencies.settings.lastTab)
    popover.excludedFrame = { [weak self] in self?.statusItem?.buttonFrameOnScreen }
    popover.onRefresh = { [weak self] in self?.refreshNow() }
    popover.onVisibilityChange = { [weak self] visible in
      self?.statusItem?.popoverVisible = visible
      self?.dependencies.state.popoverVisible = visible
      if visible {
        self?.environment.refreshUsagePresentation()
        self?.refreshIfStale()
      }
    }
    self.popover = popover
    installObservers()
    if dependencies.verificationSession != nil { writeVerificationSnapshot() }
    if let updater = dependencies.updater {
      updater.automaticallyChecks = dependencies.settings.automaticUpdates
      updater.onCanCheckChange = { [weak self] in self?.environment.canCheckForUpdates = $0 }
      updater.start()
      environment.canCheckForUpdates = updater.canCheck
    }
    dependencies.notifier.onResponse = { [weak self] in self?.handleNotificationResponse() }
    probeProviderHealth()
    providerRediscoveryPolicy.recordDiscovery(at: dependencies.clock.now())
    Task { [coordinator] in await coordinator.restoreCachedSnapshots() }
    coordinator.start()
    if notificationsEnabled { Task { await dependencies.notifier.requestAuthorization(provisional: true) } }
    observeStatusModel()
    if dependencies.openPopoverOnLaunch {
      launchPopoverTask = Task { [weak self, clock = dependencies.clock] in
        guard (try? await clock.sleep(Self.launchPopoverDelay)) != nil else { return }
        self?.openPopoverAfterStatusItemAttachment()
      }
    }
  }

  static let launchPopoverDelay: TimeInterval = 0.2
  static let attachmentPollInterval: TimeInterval = 0.05
  static let retentionDebounce: TimeInterval = 0.15

  func openPopoverAfterStatusItemAttachment(
    remainingAttempts: Int = 20, previousButtonFrame: CGRect? = nil, forcedNarrowest: Bool = false
  ) {
    launchPopoverTask?.cancel()
    updatePopoverGeometry()
    popover?.prepareToShow()
    launchPopoverTask = Task { [weak self, clock = dependencies.clock] in
      guard (try? await clock.sleep(Self.attachmentPollInterval)) != nil, let self, !stopped,
        popover?.isShown != true
      else { return }
      dependencies.log.logDebug(
        "popover attachment remaining=\(remainingAttempts) frame=\(String(describing: statusItem?.buttonFrameOnScreen)) "
          + "previous=\(String(describing: previousButtonFrame)) fits=\(statusItem?.fits() == true)")
      guard let buttonFrame = statusItem?.buttonFrameOnScreen, statusItem?.fits() == true, !buttonFrame.isEmpty,
        buttonFrame.minX.isFinite, buttonFrame.minY.isFinite,
        buttonFrame.width.isFinite, buttonFrame.height.isFinite
      else {
        retryOpening(
          remainingAttempts: remainingAttempts, previousButtonFrame: nil, forcedNarrowest: forcedNarrowest)
        return
      }
      guard buttonFrame == previousButtonFrame else {
        retryOpening(
          remainingAttempts: remainingAttempts, previousButtonFrame: buttonFrame, forcedNarrowest: forcedNarrowest)
        return
      }
      togglePopover()
    }
  }

  @discardableResult
  func retryOpening(remainingAttempts: Int, previousButtonFrame: CGRect?, forcedNarrowest: Bool) -> Bool {
    if remainingAttempts == 12, !forcedNarrowest, let statusItem {
      statusItem.popoverVisible = false
      _ = statusItem.collapseToNarrowest()
      statusItem.popoverVisible = true
      statusItem.reattach()
      openPopoverAfterStatusItemAttachment(remainingAttempts: 8, forcedNarrowest: true)
      return false
    }
    guard remainingAttempts > 1 else {
      guard forcedNarrowest, dependencies.recoversOffscreenPopover else {
        popover?.close()
        return false
      }
      togglePopover()
      return true
    }
    openPopoverAfterStatusItemAttachment(
      remainingAttempts: remainingAttempts - 1, previousButtonFrame: previousButtonFrame,
      forcedNarrowest: forcedNarrowest)
    return false
  }

  func rootView(_ popover: PopoverController) -> RootView {
    RootView(
      environment: environment, onMeasure: { [weak popover] in popover?.measure($0) },
      onTabChange: { [weak popover] tab in popover?.select(tab: tab) },
      preferredContentSize: { [weak popover] in popover?.contentSize(for: $0) },
      chooseHistoryExportURL: { [weak self] in await self?.dependencies.chooseExportURL() })
  }

  func observeStatusModel() {
    // The tracking closure re-registers itself after every change, so capturing self strongly here would leave the
    // registrar holding the controller, its state and its history store for good.
    withObservationTracking { [weak self] in
      _ = self?.dependencies.state.statusLadder
    } onChange: { [weak self] in
      Task { @MainActor [weak self] in
        guard let self, !self.stopped else { return }
        self.statusItem?.update(ladder: self.dependencies.state.statusLadder)
        self.observeStatusModel()
      }
    }
  }

  public func stop() {
    cancelBackgroundTasks()
    coordinator.stop()
    popover?.close(restoringActivation: false)
    logWindow?.close()
    logWindow = nil
    statusItem?.remove(from: dependencies.statusBar)
    statusItem = nil
    for observer in workspaceObservers { NSWorkspace.shared.notificationCenter.removeObserver(observer) }
    workspaceObservers.removeAll()
    for observer in applicationObservers { NotificationCenter.default.removeObserver(observer) }
    applicationObservers.removeAll()
    for observer in distributedObservers { DistributedNotificationCenter.default().removeObserver(observer) }
    distributedObservers.removeAll()
    verificationSnapshotRequests = nil
    dependencies.updater?.onCanCheckChange = nil
    stopped = true
    dependencies.log.log("stopped")
    dependencies.log.flush()
  }

  func cancelBackgroundTasks() {
    launchPopoverTask?.cancel()
    launchPopoverTask = nil
    retentionTask?.cancel()
    retentionTask = nil
    providerHealthTask?.cancel()
    providerHealthTask = nil
    providerRediscoveryTask?.cancel()
    providerRediscoveryTask = nil
  }

}
