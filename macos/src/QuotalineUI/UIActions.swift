import Foundation
import QuotalineCore

@MainActor
public struct UIActions {
  public var refresh: () -> Void
  public var refreshProvider: (ProviderID) -> Void
  public var signInProvider: (ProviderID) -> Void
  public var showProviders: (ProviderID?) -> Void
  public var openURL: (URL) -> Void
  public var copy: (String) -> Void
  public var exportHistory: () -> Void
  public var clearHistory: () -> Void
  public var revealHistory: () -> Void
  public var copyDiagnostics: () -> Void
  public var reportIssue: () -> Void
  public var showFullLog: () -> Void
  public var setLaunchAtLogin: (Bool) -> Void
  public var openLoginItems: () -> Void
  public var grantAccess: (SandboxResource) -> Void
  public var checkForUpdates: () -> Void
  public var quit: () -> Void
  public var setDemoMode: (Bool) -> Void
  public var settingsChanged: () -> Void
  public var historyRetentionChanged: () -> Void
  public var settingsReset: () -> Void

  public init(
    refresh: @escaping () -> Void = {},
    refreshProvider: @escaping (ProviderID) -> Void = { _ in },
    signInProvider: @escaping (ProviderID) -> Void = { _ in },
    showProviders: @escaping (ProviderID?) -> Void = { _ in },
    openURL: @escaping (URL) -> Void = { _ in },
    copy: @escaping (String) -> Void = { _ in },
    exportHistory: @escaping () -> Void = {},
    clearHistory: @escaping () -> Void = {},
    revealHistory: @escaping () -> Void = {},
    copyDiagnostics: @escaping () -> Void = {},
    reportIssue: @escaping () -> Void = {},
    showFullLog: @escaping () -> Void = {},
    setLaunchAtLogin: @escaping (Bool) -> Void = { _ in },
    openLoginItems: @escaping () -> Void = {},
    grantAccess: @escaping (SandboxResource) -> Void = { _ in },
    checkForUpdates: @escaping () -> Void = {},
    quit: @escaping () -> Void = {},
    setDemoMode: @escaping (Bool) -> Void = { _ in },
    settingsChanged: @escaping () -> Void = {},
    historyRetentionChanged: @escaping () -> Void = {},
    settingsReset: @escaping () -> Void = {}
  ) {
    self.refresh = refresh
    self.refreshProvider = refreshProvider
    self.signInProvider = signInProvider
    self.showProviders = showProviders
    self.openURL = openURL
    self.copy = copy
    self.exportHistory = exportHistory
    self.clearHistory = clearHistory
    self.revealHistory = revealHistory
    self.copyDiagnostics = copyDiagnostics
    self.reportIssue = reportIssue
    self.showFullLog = showFullLog
    self.setLaunchAtLogin = setLaunchAtLogin
    self.openLoginItems = openLoginItems
    self.grantAccess = grantAccess
    self.checkForUpdates = checkForUpdates
    self.quit = quit
    self.setDemoMode = setDemoMode
    self.settingsChanged = settingsChanged
    self.historyRetentionChanged = historyRetentionChanged
    self.settingsReset = settingsReset
  }
}

@MainActor
@Observable
public final class UIEnvironment {
  public let state: AppState
  public let settings: Settings
  public let history: UsageHistoryStore
  public let historyPresenter: HistoryPresenter
  public let spendSummary = SpendSummaryModel()
  public let disclosures = DisclosureState()
  let stagedSelection = StagedSelectionChanges()
  public let log: LogBuffer
  public let appInfo: AppInfo
  public let clock: Clock
  public var actions: UIActions
  public var launchAtLoginStatus: LaunchAtLoginBackend.Status
  public var credentialDescriptions: [ProviderID: String]
  public var canCheckForUpdates: Bool
  public var isSandboxed: Bool
  public var isDemo: Bool
  public var increaseContrast: Bool
  public var providerFocusRequest: ProviderSettingsFocusRequest?
  public var providerLoginErrors: [ProviderID: String] = [:]
  public var canLaunchProviderLogin = false
  public var firstRunCard: FirstRunCard?
  public var samples: [WindowKey: [UsageSample]] = [:]
  public var now: Date
  public private(set) var usagePresentation: UsagePresentation
  public private(set) var usageDeadlineNow: Date

  private var usageAnalyticsCache: [ProviderID: UsageAnalyticsCacheEntry]
  private var usagePresentationDirty = false
  private var recentSamplesLoad: Task<Void, Never>?
  private var settingsActivityCache: SettingsActivityCacheEntry?
  private(set) var settingsActivityLoad: SettingsActivityLoad?
  @ObservationIgnored private var tabTransition: (tab: PopoverTab, startedAt: TimeInterval)?
  @ObservationIgnored weak var tabContainer: PersistentTabContainer?

  public init(
    state: AppState,
    settings: Settings,
    history: UsageHistoryStore,
    log: LogBuffer,
    appInfo: AppInfo,
    clock: Clock = .system,
    actions: UIActions = UIActions(),
    launchAtLoginStatus: LaunchAtLoginBackend.Status = .unknown,
    credentialDescriptions: [ProviderID: String] = [:],
    canCheckForUpdates: Bool = false,
    isSandboxed: Bool = false,
    isDemo: Bool = false,
    increaseContrast: Bool = false
  ) {
    self.state = state
    self.settings = settings
    self.history = history
    self.log = log
    self.appInfo = appInfo
    self.clock = clock
    self.actions = actions
    self.launchAtLoginStatus = launchAtLoginStatus
    self.credentialDescriptions = credentialDescriptions
    self.canCheckForUpdates = canCheckForUpdates
    self.isSandboxed = isSandboxed
    self.isDemo = isDemo
    self.increaseContrast = increaseContrast
    providerFocusRequest = nil
    historyPresenter = HistoryPresenter(
      history: history,
      settings: settings,
      clock: clock,
      initialScope: HistoryDataScope(
        activeProviders: settings.activeProviders(states: state.providers),
        selectedWindows: Set(settings.modelSelection(in: state.snapshots))),
      persistMetric: { settings.historyMetricID = $0.storageID })
    let now = clock.now()
    self.now = now
    usageDeadlineNow = now
    settingsActivityCache = nil
    settingsActivityLoad = nil
    let analyticsCache = Self.analyticsCache(state.providers, now: now)
    usageAnalyticsCache = analyticsCache
    let activeProviders = settings.activeProviders(states: state.providers)
    usagePresentation = UsagePresenter.presentation(
      state: state.providers, enabled: activeProviders,
      selected: Set(settings.modelSelection(in: state.snapshots)),
      samples: [:], analytics: analyticsCache.mapValues(\.presentation), lastRefresh: state.lastRefresh,
      iconTone: UsagePresenter.iconTone(state.providers.filter { activeProviders.contains($0.key) }),
      isRefreshing: state.isRefreshing, options: settings.presentationOptions, now: now)
    observeUsageInputs()
  }

  public func tick() {
    let now = clock.now()
    self.now = now
    advanceUsageDeadlines(to: now)
  }

  public func beginTabTransition(to tab: PopoverTab, inputTimestamp: TimeInterval? = nil) {
    let now = ProcessInfo.processInfo.systemUptime
    let started = inputTimestamp ?? now
    tabTransition = (tab, started)
    log.detailed(
      .tab(
        TabDiagnostic(
          action: .transition, from: settings.lastTab.rawValue, to: tab.rawValue,
          activeTab: settings.lastTab.rawValue, durationMilliseconds: max((now - started) * 1_000, 0))))
  }

  public func completeTabTransition(to tab: PopoverTab) {
    guard let transition = tabTransition, transition.tab == tab else { return }
    tabTransition = nil
    log.detailed(
      .tab(
        TabDiagnostic(
          action: .presented,
          to: tab.rawValue,
          activeTab: tab.rawValue,
          durationMilliseconds: max(
            (ProcessInfo.processInfo.systemUptime - transition.startedAt) * 1_000,
            0))))
  }

  public func refreshUsagePresentation(at date: Date? = nil) {
    let now = date ?? clock.now()
    self.now = now
    usageDeadlineNow = now
    updateAnalyticsCache(now: now)
    let activeProviders = settings.activeProviders(states: state.providers)
    usagePresentation = UsagePresenter.presentation(
      state: state.providers, enabled: activeProviders,
      selected: Set(settings.modelSelection(in: state.snapshots)),
      samples: samples, analytics: usageAnalyticsCache.mapValues(\.presentation), lastRefresh: state.lastRefresh,
      iconTone: UsagePresenter.iconTone(state.providers.filter { activeProviders.contains($0.key) }),
      isRefreshing: state.isRefreshing, options: settings.presentationOptions, now: now)
    usagePresentationDirty = false
  }

  public func nextUsageDeadline(after date: Date? = nil) -> Date? {
    usagePresentation.nextDeadline(after: date ?? usageDeadlineNow)
  }

  @discardableResult
  public func advanceUsageDeadlines(to date: Date? = nil) -> Bool {
    let date = date ?? clock.now()
    guard let deadline = nextUsageDeadline(), date >= deadline else { return false }
    usageDeadlineNow = date
    return true
  }

  public func prepareUsage() async {
    if usagePresentationDirty { refreshUsagePresentation() }
    await loadRecentSamples()
  }

  public func loadRecentSamples(force: Bool = false) async {
    // Only the popover's cards read these, and one query covers every window: a loop was an actor hop and a
    // prepared statement each.
    guard force || (state.popoverVisible && settings.lastTab == .usage) else { return }
    if let recentSamplesLoad {
      await recentSamplesLoad.value
      return
    }
    let since = now.addingTimeInterval(-PaceEstimate.slopeWindow)
    var keys: [WindowKey] = []
    for (provider, item) in state.providers {
      guard let windows = item.snapshot?.windows else { continue }
      for window in windows { keys.append(WindowKey(provider, window)) }
    }
    let task = Task { [history] in
      let rows = (try? await history.samples(keys: keys, from: since, to: .distantFuture)) ?? []
      var loaded = Dictionary(grouping: rows, by: \.key)
      for key in keys where loaded[key] == nil { loaded[key] = [] }
      samples = loaded
      if force || (state.popoverVisible && settings.lastTab == .usage) {
        refreshUsagePresentation()
      } else {
        usagePresentationDirty = true
      }
      recentSamplesLoad = nil
    }
    recentSamplesLoad = task
    await task.value
  }

  func settingsActivity(for request: SettingsActivityRequest) async -> [WindowKey: Date] {
    if let cache = settingsActivityCache, cache.request == request { return cache.dates }
    if let load = settingsActivityLoad, load.request == request { return await load.task.value ?? [:] }
    settingsActivityLoad?.task.cancel()
    let id = UUID()
    let end = clock.now()
    let start = end.addingTimeInterval(-TimeInterval(request.retentionDays) * 86400)
    let task = Task { [history] in
      try? await history.lastUsageDates(keys: request.keys, from: start, to: end)
    }
    settingsActivityLoad = SettingsActivityLoad(id: id, request: request, task: task)
    let dates = await task.value
    let resolvedDates: [WindowKey: Date] = if let dates { dates } else { [:] }
    guard settingsActivityLoad?.id == id else { return resolvedDates }
    settingsActivityLoad = nil
    if let dates { settingsActivityCache = SettingsActivityCacheEntry(request: request, dates: dates) }
    return resolvedDates
  }

  public var cards: [ProviderCard] {
    usagePresentation.cards
  }

  private func observeUsageInputs() {
    withObservationTracking {
      _ = state.providers
      _ = state.lastRefresh
      _ = state.isRefreshing
      _ = settings.enabledProviders
      _ = settings.configuredProviders
      _ = settings.selectedWindows
      _ = settings.hasCustomSelection
      _ = settings.usageDisplay
      _ = settings.paceWorkdays
      _ = settings.hidePersonalInformation
    } onChange: { [weak self] in
      Task { @MainActor [weak self] in
        guard let self else { return }
        if self.state.popoverVisible,
          self.settings.lastTab == .usage || !self.state.isRefreshing
        {
          self.refreshUsagePresentation()
          self.tabContainer?.refresh(.usage)
        } else {
          self.usagePresentationDirty = true
        }
        self.observeUsageInputs()
      }
    }
  }

  private func updateAnalyticsCache(now: Date) {
    let day = DayStamp.string(now)
    let providers = Set(state.providers.keys)
    usageAnalyticsCache = usageAnalyticsCache.filter { providers.contains($0.key) }
    for (provider, providerState) in state.providers {
      guard let analytics = providerState.analytics else {
        usageAnalyticsCache[provider] = nil
        continue
      }
      let key = UsageAnalyticsCacheKey(fetchedAt: analytics.fetchedAt, day: day)
      guard usageAnalyticsCache[provider]?.key != key else { continue }
      usageAnalyticsCache[provider] = UsageAnalyticsCacheEntry(
        key: key, presentation: UsagePresenter.analyticsPresentation(analytics, now: now))
    }
  }

  private static func analyticsCache(
    _ state: [ProviderID: ProviderState], now: Date
  ) -> [ProviderID: UsageAnalyticsCacheEntry] {
    let day = DayStamp.string(now)
    return state.compactMapValues { providerState in
      providerState.analytics.map {
        UsageAnalyticsCacheEntry(
          key: UsageAnalyticsCacheKey(fetchedAt: $0.fetchedAt, day: day),
          presentation: UsagePresenter.analyticsPresentation($0, now: now))
      }
    }
  }
}

private struct UsageAnalyticsCacheKey: Equatable {
  let fetchedAt: Date
  let day: String
}

private struct UsageAnalyticsCacheEntry {
  let key: UsageAnalyticsCacheKey
  let presentation: UsageAnalyticsPresentation
}

private struct SettingsActivityCacheEntry {
  let request: SettingsActivityRequest
  let dates: [WindowKey: Date]
}

struct SettingsActivityLoad {
  let id: UUID
  let request: SettingsActivityRequest
  let task: Task<[WindowKey: Date]?, Never>
}
