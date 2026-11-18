import Foundation
import Observation
import QuotalineRust

private struct RefreshRun {
  let generation: Int
  let registry: ProviderRegistry
  let task: Task<Void, Never>
}

@MainActor
public final class RefreshCoordinator {
  private static let refreshLimits = QuotalineRust.refreshLimits()
  public static let rateLimitBackoff: TimeInterval = refreshLimits.rateLimitBackoff
  public static let minimumBackoff: TimeInterval = refreshLimits.minimumBackoff
  public static let maximumBackoff: TimeInterval = refreshLimits.maximumBackoff
  public static let networkBackoff: TimeInterval = refreshLimits.networkBackoff
  public static let wakeDelay: TimeInterval = 3
  public static let idleThreshold: TimeInterval = refreshLimits.idleThreshold

  public private(set) var registry: ProviderRegistry
  private let settings: Settings
  private let state: AppState
  private let history: UsageHistoryStore
  private let log: LogBuffer
  private let clock: Clock
  private let publicationClock: Clock
  private let powerState: @Sendable () -> PowerState
  private let session: QuotalineRust.RefreshSession
  private let notify: @MainActor ([NotificationEvent]) -> Void
  private var loop: Task<Void, Never>?
  private let refreshesAutomatically: Bool
  private var refreshRun: RefreshRun?
  private var historyClearTask: Task<Int, any Error>?
  private var activeGeneration: Int?
  private let notificationDelivery = QuotalineRust.NotificationDeliveryLedger()
  private var refreshGeneration = 0
  private var registryGeneration = 0
  private var scheduleObservationGeneration = 0
  private let persistence: SnapshotPersistence
  private let applier: ProviderRefreshApplier
  private let statusPublisher: StatusPublisher
  private var cacheSubmissionTask: Task<Void, Never>?
  public var widgetSink: ((WidgetSnapshot) -> Void)? {
    get { statusPublisher.widgetSink }
    set { statusPublisher.widgetSink = newValue }
  }

  public init(
    registry: ProviderRegistry,
    settings: Settings,
    state: AppState,
    history: UsageHistoryStore,
    log: LogBuffer,
    clock: Clock = .system,
    publicationClock: Clock = .system,
    cache: SnapshotCache = SnapshotCache(url: nil),
    persistence: SnapshotPersistence? = nil,
    powerState: @escaping @Sendable () -> PowerState = PowerState.current,
    refreshesAutomatically: Bool = true,
    notify: @escaping @MainActor ([NotificationEvent]) -> Void
  ) {
    self.refreshesAutomatically = refreshesAutomatically
    self.registry = registry
    self.settings = settings
    self.state = state
    self.history = history
    self.log = log
    self.clock = clock
    self.publicationClock = publicationClock
    self.powerState = powerState
    session = QuotalineRust.RefreshSession(now: clock.now().timeIntervalSince1970)
    self.persistence =
      persistence
      ?? SnapshotPersistence(
        cache: cache,
        failureHandler: { failure in log.logError(failure.message) })
    self.notify = notify
    statusPublisher = StatusPublisher(settings: settings, state: state, clock: clock)
    applier = ProviderRefreshApplier(
      settings: settings, state: state, history: history, log: log, session: session,
      notificationDelivery: notificationDelivery)
    rebuildStatus()
  }

  public func restoreCachedSnapshots() async {
    let snapshots = await persistence.loadSnapshots()
    var restored: [ProviderID: ProviderState] = [:]
    for (provider, snapshot) in snapshots where registry[provider] != nil {
      guard state.state(for: provider).snapshot == nil else { continue }
      var providerState = state.state(for: provider)
      providerState.snapshot = snapshot
      providerState.availability = .stale
      restored[provider] = providerState
    }
    if !restored.isEmpty {
      state.applyProviderStates(restored)
      rebuildStatus()
    }
  }

  public var isRunning: Bool {
    loop != nil
  }

  public func start() {
    start(after: 0)
  }

  public func resume() {
    start(after: Self.wakeDelay)
  }

  private func start(after delay: TimeInterval) {
    guard loop == nil else { return }
    scheduleObservationGeneration &+= 1
    let generation = scheduleObservationGeneration
    loop = makeLoop(initialDelay: delay)
    observeScheduleInputs(generation: generation)
  }

  public func stop() {
    scheduleObservationGeneration &+= 1
    loop?.cancel()
    loop = nil
    refreshRun?.task.cancel()
    refreshRun = nil
    session.cancel()
    activeGeneration = nil
    state.setNextRefresh(nil)
    state.cancelRefreshing()
  }

  public func refresh(_ request: RefreshRequest) async {
    if let historyClearTask { _ = await historyClearTask.result }
    guard !Task.isCancelled else { return }
    session.request(intent: request.rustIntent)
    while session.hasPending() || refreshRun != nil {
      if let run = refreshRun {
        await run.task.value
        if refreshRun?.generation == run.generation { refreshRun = nil }
        continue
      }
      refreshGeneration += 1
      let generation = refreshGeneration
      let registry = registry
      let registryGeneration = registryGeneration
      let task = Task {
        await drainRefreshes(generation: generation, registry: registry, registryGeneration: registryGeneration)
      }
      refreshRun = RefreshRun(generation: generation, registry: registry, task: task)
    }
  }

  public func clearHistory() async throws -> Int {
    if let historyClearTask { return try await historyClearTask.value }
    let wasRunning = loop != nil
    let runningRefresh = refreshRun?.task
    stop()
    let stoppedGeneration = scheduleObservationGeneration
    let task = Task {
      await runningRefresh?.value
      let removed = try await history.clear()
      for provider in registry.providers { await provider.resetAnalyticsHistory() }
      session.forgetAnalyticsAttempts()
      for provider in registry.ids {
        state.update(provider) {
          $0.analytics = nil
          $0.lastAnalyticsAttempt = nil
        }
      }
      notificationDelivery.clearPace()
      state.markSamplesChanged()
      return removed
    }
    historyClearTask = task
    defer {
      historyClearTask = nil
      if wasRunning, scheduleObservationGeneration == stoppedGeneration {
        start(after: registry.providers.map(\.pollingPolicy.minimumInterval).min() ?? Self.minimumBackoff)
      }
    }
    return try await task.value
  }

  public func replaceRegistry(_ registry: ProviderRegistry) {
    let previous = self.registry
    self.registry = registry
    registryChanged(from: previous)
  }

  private func registryChanged(from previous: ProviderRegistry) {
    let removed = Set(previous.ids).subtracting(registry.ids)
    registryGeneration += 1
    refreshRun?.task.cancel()
    state.cancelRefreshing()
    state.applySetupStates(registry.setupStates)
    state.removeProviders(removed)
    session.retainProviders(providers: registry.ids.map(\.rawValue))
    notificationDelivery.retainProviders(providers: registry.ids.map(\.rawValue))
    reschedule()
  }

  public func reschedule() {
    guard loop != nil, refreshesAutomatically else { return }
    loop?.cancel()
    loop = makeLoop()
  }

  public func nextRefreshDate(now: Date? = nil) -> Date? {
    session.nextDeadline(inputs: scheduleInputs(registry, now: now ?? clock.now()))
      .map(Date.init(timeIntervalSince1970:))
  }

  private func drainRefreshes(
    generation: Int,
    registry: ProviderRegistry,
    registryGeneration: Int
  ) async {
    while !Task.isCancelled, let cycle = session.start() {
      activeGeneration = generation
      await perform(cycle, registry: registry, registryGeneration: registryGeneration)
      session.finish(cycle: cycle)
      if activeGeneration == generation { activeGeneration = nil }
    }
  }

  private func makeLoop(initialDelay: TimeInterval = 0) -> Task<Void, Never> {
    Task { [weak self] in
      guard let self, !Task.isCancelled else { return }
      if initialDelay > 0 {
        state.setNextRefresh(clock.now().addingTimeInterval(initialDelay))
        do {
          try await clock.sleep(initialDelay)
        } catch {
          return
        }
      }
      while !Task.isCancelled {
        await refresh(RefreshRequest())
        guard refreshesAutomatically, !Task.isCancelled, let deadline = nextRefreshDate() else { break }
        state.setNextRefresh(deadline)
        do {
          try await clock.sleep(max(0, deadline.timeIntervalSince(clock.now())))
        } catch {
          break
        }
      }
    }
  }

  private func observeScheduleInputs(generation: Int) {
    withObservationTracking {
      _ = state.popoverVisible
      _ = settings.enabledProviders
      _ = settings.configuredProviders
      _ = settings.refreshSeconds
      _ = settings.analyticsRefreshMinutes
    } onChange: { [weak self] in
      Task { @MainActor [weak self] in
        guard let self, self.loop != nil, self.scheduleObservationGeneration == generation else { return }
        if self.state.popoverVisible {
          self.session.markVisible(now: self.clock.now().timeIntervalSince1970)
        }
        self.observeScheduleInputs(generation: generation)
        self.reschedule()
      }
    }
  }

  private func perform(
    _ cycle: QuotalineRust.RefreshCycle,
    registry: ProviderRegistry,
    registryGeneration: Int
  ) async {
    let request = RefreshRequest(cycle.intent)
    let now = clock.now()
    let cycleID = log.debugEnabled ? UUID().uuidString : ""
    let active = settings.activeProviders(states: state.providers)
    let inactive = registry.ids.filter { !active.contains($0) }
    let planned = session.plan(cycle: cycle, inputs: scheduleInputs(registry, now: now))
    for skipped in planned.skipped {
      let provider = ProviderID(rawValue: skipped.provider)!
      log.detailed(
        .refresh(
          RefreshDiagnostic.skipped(
            cycleID: cycleID,
            trigger: request.reason.diagnosticName,
            provider: provider,
            usagePolicy: request.usage.diagnosticName,
            analyticsPolicy: request.analytics.diagnosticName,
            reason: skipReason(skipped.skip, for: provider))))
    }
    let plans = planned.fetches.compactMap { fetch in
      registry[ProviderID(rawValue: fetch.provider)!].map { (provider: $0, fetch: fetch) }
    }
    guard !plans.isEmpty, !Task.isCancelled else {
      state.disable(inactive)
      return
    }
    let planIDs = plans.map(\.provider.id)
    state.beginRefreshing(planIDs, disabling: inactive)
    var completed = false
    defer {
      if registryGeneration == self.registryGeneration {
        state.finishRefreshing(planIDs, at: completed ? clock.now() : nil)
      }
    }
    var samplesChanged = false
    var analyticsChanged = false
    let generation = refreshGeneration
    let publication = ProviderPublication(clock: publicationClock) { [weak self] providers, events in
      guard let self, activeGeneration == generation, registryGeneration == self.registryGeneration else { return }
      state.applyProviderStates(providers)
      rebuildStatus(now: clock.now())
      let eventsByProvider = Dictionary(grouping: events, by: \.provider)
      let notifications = providers.flatMap { id, providerState in
        notificationDelivery.filtered(
          eventsByProvider[id] ?? [], for: id, credential: providerState.credentialHealth,
          snapshot: providerState.snapshot)
      }
      if !notifications.isEmpty { notify(notifications) }
    }
    defer { publication.cancel() }
    await withTaskGroup(
      of: (ProviderID, ProviderFetchResult, ProviderApplyResult, Bool, Int).self
    ) { group in
      for plan in plans {
        let provider = plan.provider
        let options = FetchOptions(
          includeAnalytics: plan.fetch.includeAnalytics, analyticsDays: settings.historyRetentionDays)
        group.addTask {
          let clock = ContinuousClock()
          let started = clock.now
          let result = await DiagnosticSignposts.refresh.withInterval("Provider refresh") {
            await provider.fetch(now: now, options: options)
          }
          let credentialHealth: ProviderCredentialHealth
          if let fetched = result.credentialHealth {
            credentialHealth = fetched
          } else {
            credentialHealth = await provider.credentialHealth(now: now)
          }
          let duration = Self.milliseconds(started.duration(to: clock.now))
          // Serial persistence lets SwiftUI lay out between each ready provider result.
          let applied = await self.applier.apply(
            provider.id,
            result: result,
            credentialHealth: credentialHealth,
            includedAnalytics: options.includeAnalytics,
            retryInterval: plan.fetch.retryInterval,
            generation: cycle.generation,
            isCurrent: { registryGeneration == self.registryGeneration && self.registry[provider.id] != nil },
            now: now)
          return (
            provider.id,
            result,
            applied,
            options.includeAnalytics,
            duration
          )
        }
      }
      for await (id, result, applied, includedAnalytics, duration) in group {
        guard !Task.isCancelled else {
          group.cancelAll()
          break
        }
        if let providerState = applied.providerState {
          publication.append(id, state: providerState, events: applied.events)
        }
        samplesChanged = applied.samplesChanged || samplesChanged
        analyticsChanged = applied.analyticsChanged || analyticsChanged
        log.detailed(
          .refresh(
            RefreshDiagnostic(
              cycleID: cycleID,
              trigger: request.reason.diagnosticName,
              provider: id,
              usagePolicy: request.usage.diagnosticName,
              analyticsPolicy: request.analytics.diagnosticName,
              outcome: result.outcome.diagnosticOutcome,
              durationMilliseconds: duration,
              includeAnalytics: includedAnalytics,
              analyticsReturned: result.analytics != nil,
              analyticsPointCount: result.analytics?.points.count ?? 0,
              warnings: result.warnings)))
      }
    }
    guard !Task.isCancelled, registryGeneration == self.registryGeneration else { return }
    publication.flush()
    if samplesChanged {
      state.markSamplesChanged()
    } else if analyticsChanged {
      state.markHistoryChanged()
    }
    storeCache()
    completed = true
  }

  nonisolated private static func milliseconds(_ duration: Duration) -> Int {
    let components = duration.components
    return Int(components.seconds * 1_000 + components.attoseconds / 1_000_000_000_000_000)
  }

  func storeCache() {
    let snapshots = state.snapshots
    let writesUsageFile = settings.writeUsageFile
    let previous = cacheSubmissionTask
    cacheSubmissionTask = Task { [persistence] in
      await previous?.value
      await persistence.submitSnapshots(snapshots, writesUsageFile: writesUsageFile)
    }
  }

  public func flushPersistence() async {
    await cacheSubmissionTask?.value
    await persistence.flush()
  }

  private func skipReason(_ skip: QuotalineRust.RefreshSkip, for id: ProviderID) -> DiagnosticRefreshSkipReason {
    switch skip {
    case .inactive: return settings.providerOverride(for: id) == false ? .disabled : .notDiscovered
    case .retryBackoff: return .retryBackoff
    case .analyticsNotDue: return .analyticsNotDue
    case .noWork: return .noWork
    }
  }

  private func scheduleInputs(
    _ registry: ProviderRegistry, now: Date
  ) -> QuotalineRust.RefreshScheduleInputs {
    // Provider order sets each provider's stagger phase, so it follows the catalog rather than the registry.
    let providers = ProviderID.allCases.compactMap { registry[$0] }
    return QuotalineRust.RefreshScheduleInputs(
      providers: providers.map { provider in
        QuotalineRust.RefreshProviderSchedule(
          id: provider.id.rawValue,
          active: settings.isProviderActive(provider.id, state: state.state(for: provider.id)),
          minimum: provider.pollingPolicy.minimumInterval, activeInterval: provider.pollingPolicy.activeInterval,
          requested: TimeInterval(settings.refreshInterval(for: provider.id)),
          supportsAnalytics: provider.supportsAnalytics)
      },
      visible: state.popoverVisible, throttled: powerState().throttlesScanning,
      analyticsInterval: TimeInterval(settings.analyticsRefreshMinutes * 60),
      maximum: TimeInterval(Settings.maximumRefreshSeconds), idleThreshold: Self.idleThreshold,
      now: now.timeIntervalSince1970)
  }

  public func nextAttempt(for id: ProviderID) -> Date? {
    state.state(for: id).retryNotBefore
  }

  public func rebuildStatus(now: Date? = nil, publishWidget: Bool = true) {
    statusPublisher.rebuildStatus(now: now, publishWidget: publishWidget)
  }
}
