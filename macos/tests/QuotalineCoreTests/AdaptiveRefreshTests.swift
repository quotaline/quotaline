import Foundation
import QuotalineTestSupport
import Testing

@testable import QuotalineCore

private let lowPower = PowerState(lowPowerMode: true, thermalPressure: false)

// Cursor has no analytics clock, so the next refresh date reflects the usage interval alone.
@MainActor
private func makeAdaptiveCoordinator(
  interval: Int = 300, clock: Clock = testClock, power: PowerSwitch = PowerSwitch(),
  persistence: SnapshotPersistence? = nil, settings: Settings? = nil, refreshesAutomatically: Bool = true
) throws -> (RefreshCoordinator, AppState, ScriptedProvider) {
  let settings = settings ?? Settings(defaults: testDefaults())
  settings.setProvider(.cursor, enabled: true)
  settings.setRefreshInterval(interval, for: .cursor)
  let provider = ScriptedProvider(
    id: .cursor,
    results: [
      ProviderFetchResult(
        outcome: .success(
          ProviderReading(
            provider: .cursor,
            windows: [
              QuotaWindow(
                id: "plan", label: "Plan", group: .monthly, usedPercent: 40, resetsAt: nil, duration: nil)
            ], fetchedAt: fixedNow)))
    ])
  provider.pollingPolicy = PollingPolicy(minimumInterval: 60, activeInterval: 120, defaultInterval: 300)
  let state = AppState()
  state.update(.cursor) { $0.credentialHealth = .signedIn(.cursor) }
  let coordinator = RefreshCoordinator(
    registry: ProviderRegistry([provider]), settings: settings, state: state,
    history: try UsageHistoryStore(url: nil), log: makeLog(), clock: clock, persistence: persistence,
    powerState: { power.current }, refreshesAutomatically: refreshesAutomatically
  ) { _ in }
  return (coordinator, state, provider)
}

@Test @MainActor func aCoordinatorWithoutAutomaticRefreshPollsOnlyAtStartAndOnRequest() async throws {
  let recorder = SleepRecorder()
  let clock = Clock(now: { fixedNow }, sleep: { try await recorder.sleep($0) })
  let (coordinator, state, provider) = try makeAdaptiveCoordinator(clock: clock, refreshesAutomatically: false)
  coordinator.start()
  defer { coordinator.stop() }
  while provider.callCount < 1 { await Task.yield() }

  state.popoverVisible = true
  coordinator.reschedule()
  for _ in 0..<100 { await Task.yield() }
  #expect(provider.callCount == 1)
  #expect(await recorder.durations.isEmpty)
  #expect(state.nextRefreshAt == nil)

  await coordinator.refresh(RefreshRequest(reason: .userInitiated, usage: .force))
  #expect(provider.callCount == 2)
}

@Test(arguments: throttledPowerStates)
@MainActor func scheduledRefreshWaitsTwiceAsLongUnderPowerPressure(_ pressure: PowerState) async throws {
  let box = DateBox(fixedNow)
  let power = PowerSwitch()
  let (coordinator, _, provider) = try makeAdaptiveCoordinator(clock: box.clock, power: power)
  await coordinator.refresh(RefreshRequest())
  #expect(coordinator.nextRefreshDate() == fixedNow.addingTimeInterval(300))

  power.current = pressure
  #expect(coordinator.nextRefreshDate() == fixedNow.addingTimeInterval(600))
  box.date = fixedNow.addingTimeInterval(300)
  await coordinator.refresh(RefreshRequest())
  #expect(provider.callCount == 1)
  await coordinator.refresh(RefreshRequest(reason: .userInitiated, usage: .force))
  #expect(provider.callCount == 2)
  box.date = fixedNow.addingTimeInterval(900)
  await coordinator.refresh(RefreshRequest())
  #expect(provider.callCount == 3)
}

@Test @MainActor func scheduledRefreshWaitsThreeTimesAsLongAfterAnHourWithoutThePopover() async throws {
  let box = DateBox(fixedNow)
  let power = PowerSwitch()
  let (coordinator, _, _) = try makeAdaptiveCoordinator(interval: 600, clock: box.clock, power: power)
  await coordinator.refresh(RefreshRequest())

  box.date = fixedNow.addingTimeInterval(RefreshCoordinator.idleThreshold - 1)
  #expect(coordinator.nextRefreshDate() == fixedNow.addingTimeInterval(600))
  box.date = fixedNow.addingTimeInterval(RefreshCoordinator.idleThreshold)
  #expect(coordinator.nextRefreshDate() == fixedNow.addingTimeInterval(1800))
  power.current = lowPower
  #expect(coordinator.nextRefreshDate() == fixedNow.addingTimeInterval(TimeInterval(Settings.maximumRefreshSeconds)))
}

@Test @MainActor func scheduledRefreshNeverExceedsTheMaximumInterval() async throws {
  let power = PowerSwitch()
  power.current = lowPower
  let (coordinator, _, _) = try makeAdaptiveCoordinator(interval: Settings.maximumRefreshSeconds, power: power)
  await coordinator.refresh(RefreshRequest())
  #expect(coordinator.nextRefreshDate() == fixedNow.addingTimeInterval(TimeInterval(Settings.maximumRefreshSeconds)))
}

@Test @MainActor func openingThePopoverResetsTheIdleMultiplier() async throws {
  let box = DateBox(fixedNow)
  let recorder = SleepRecorder()
  let clock = Clock(now: { box.date }, sleep: { try await recorder.sleep($0) })
  let (coordinator, state, provider) = try makeAdaptiveCoordinator(interval: 600, clock: clock)
  coordinator.start()
  defer { coordinator.stop() }
  while await recorder.durations.count < 1 { await Task.yield() }
  #expect(await recorder.durations == [600])

  box.date = fixedNow.addingTimeInterval(RefreshCoordinator.idleThreshold)
  coordinator.reschedule()
  while await recorder.durations.count < 2 { await Task.yield() }
  #expect(await recorder.durations == [600, 1800])
  #expect(provider.callCount == 2)

  state.popoverVisible = true
  while await recorder.durations.count < 3 { await Task.yield() }
  #expect(await recorder.durations == [600, 1800, 120])
  state.popoverVisible = false
  while await recorder.durations.count < 4 { await Task.yield() }
  #expect(await recorder.durations == [600, 1800, 120, 600])
  #expect(provider.callCount == 2)
}

@Test(arguments: [true, false])
@MainActor func coordinatorWritesTheUsageFileWhenTheSettingIsOn(enabled: Bool) async throws {
  let root = temporaryDirectory()
  let usageURL = root.appendingPathComponent(UsageExport.fileName)
  let persistence = SnapshotPersistence(
    cache: SnapshotCache(url: root.appendingPathComponent("snapshots.json")), usageFileURL: usageURL)
  let settings = Settings(defaults: testDefaults())
  settings.writeUsageFile = enabled
  let (coordinator, _, _) = try makeAdaptiveCoordinator(persistence: persistence, settings: settings)
  await coordinator.refresh(RefreshRequest())
  await coordinator.flushPersistence()
  #expect(FileManager.default.fileExists(atPath: usageURL.path) == enabled)
  #expect(FileManager.default.fileExists(atPath: root.appendingPathComponent("snapshots.json").path))
}
