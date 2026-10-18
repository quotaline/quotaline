import AppKit
import Darwin
import QuotalineTestSupport
import Testing
import UserNotifications

@testable import QuotalineCore
@testable import QuotalineUI

@Test @MainActor func reopenWaitsForTheStatusItemBeforeShowingThePopover() async throws {
  let clock = ManualClock()
  let provider = ScriptedProvider(id: .claude, result: ProviderFetchResult(outcome: .success(sampleSnapshot(.claude))))
  let (dependencies, _) = try makeDependencies(providers: [provider], clock: clock.clock)
  let controller = AppController(dependencies: dependencies)
  controller.start()
  defer { controller.stop() }

  controller.reopen()

  await clock.advance(by: AppController.attachmentPollInterval) {
    clock.sleeps.contains(AppController.attachmentPollInterval)
  }
  #expect(controller.popover?.isShown == false)
}

@Test @MainActor func openingANotificationSelectsTheUsageTabAndReopens() throws {
  let (controller, dependencies, _) = try startedController()
  defer { controller.stop() }
  dependencies.settings.lastTab = .history

  dependencies.notifier.onResponse?()

  #expect(dependencies.settings.lastTab == .usage)
  #expect(dependencies.log.text.contains("notification opened"))
}

@Test @MainActor func popoverScreenLookupPrefersTheWindowThenTheAnchor() throws {
  let screen = try #require(NSScreen.screens.first)
  let inside = screen.frame.insetBy(dx: 10, dy: 10)
  let justAbove = CGRect(x: screen.frame.midX, y: screen.frame.maxY + 50, width: 10, height: 10)
  let far = CGRect(x: -100_000, y: -100_000, width: 10, height: 10)

  #expect(AppController.screen(near: nil, preferred: screen, among: []) === screen)
  #expect(AppController.screen(near: nil, preferred: nil, among: [screen]) == nil)
  #expect(AppController.screen(near: inside, preferred: nil, among: [screen]) === screen)
  #expect(AppController.screen(near: justAbove, preferred: nil, among: [screen]) === screen)
  #expect(AppController.screen(near: far, preferred: nil, among: [screen]) == nil)
}

@Test @MainActor func enablingNotificationsLaterRequestsAuthorization() async throws {
  let (dependencies, _) = try makeDependencies()
  dependencies.settings.notifications.enabled = false
  let controller = AppController(dependencies: dependencies)
  controller.start()
  defer { controller.stop() }
  await mainActorTurn()
  #expect(!dependencies.notifier.authorized)

  dependencies.settings.notifications.enabled = true
  controller.settingsChanged()

  await waitUntil { dependencies.notifier.authorized }
  #expect(dependencies.notifier.authorized)
}

@Test(arguments: [false, true]) @MainActor
func appControllerAppliesDemoModeBeforeRelaunch(enabled: Bool) throws {
  let (dependencies, recorder) = try makeDependencies()
  let controller = AppController(dependencies: dependencies)

  controller.setDemoMode(enabled)

  #expect(dependencies.settings.demoMode == enabled)
  #expect(controller.environment.isDemo == enabled)
  #expect(dependencies.log.text.contains("demo mode \(enabled ? "on" : "off"); relaunching"))
  #expect(recorder.relaunched == 1)
}

@MainActor
private final class ContrastFlag {
  var value = false
}

@Test @MainActor func appControllerFollowsTheIncreaseContrastSetting() async throws {
  var (dependencies, _) = try makeDependencies()
  let flag = ContrastFlag()
  dependencies.increaseContrast = { flag.value }
  let controller = AppController(dependencies: dependencies)
  controller.start()
  defer { controller.stop() }
  #expect(!controller.environment.increaseContrast)

  flag.value = true
  NSWorkspace.shared.notificationCenter.post(
    name: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, object: nil)

  await waitUntil { controller.environment.increaseContrast }
}

@Test @MainActor func appControllerDeliversNotificationsWithTheCurrentSoundSetting() async throws {
  let provider = ScriptedProvider(id: .claude, result: ProviderFetchResult(outcome: .notAuthenticated("expired")))
  let (dependencies, _) = try makeDependencies(providers: [provider])
  dependencies.settings.notifications.playSound = false
  let controller = AppController(dependencies: dependencies)
  controller.start()
  defer { controller.stop() }

  await waitUntil { dependencies.notifier.delivered.contains { $0.kind == .authentication } }
  #expect(!dependencies.notifier.playSound)
}

@Test @MainActor func retryOpeningCollapsesTheStatusItemOnTheTwelfthAttempt() throws {
  let (controller, _, _) = try startedController()
  defer { controller.stop() }

  #expect(!controller.retryOpening(remainingAttempts: 12, previousButtonFrame: nil, forcedNarrowest: false))
}

@Test @MainActor func cancelledProviderRediscoveryKeepsTheCurrentProviders() async throws {
  let (gate, release) = AsyncStream<Void>.makeStream()
  let (dependencies, _) = try makeDependencies(rebuildProviders: { _ in
    for await _ in gate { break }
    return ProviderRegistry([
      ScriptedProvider(id: .codex, result: ProviderFetchResult(outcome: .success(sampleSnapshot(.codex))))
    ])
  })
  let controller = AppController(dependencies: dependencies)
  let before = controller.registry.ids
  let rediscovery = Task { await controller.rediscoverProviders(.userInitiated) }
  await waitUntil { controller.providerRediscoveryTask != nil }

  controller.cancelBackgroundTasks()
  release.yield()
  await rediscovery.value

  #expect(controller.registry.ids == before)
}

@Test @MainActor func userRediscoveryWaitsForAndRepeatsAnActiveDiscovery() async throws {
  let (gate, release) = AsyncStream<Void>.makeStream()
  let (dependencies, recorder) = try makeDependencies(rebuildProviders: { _ in
    for await _ in gate { break }
    return ProviderRegistry([
      ScriptedProvider(id: .codex, result: ProviderFetchResult(outcome: .success(sampleSnapshot(.codex))))
    ])
  })
  let controller = AppController(dependencies: dependencies)
  let activation = Task { await controller.rediscoverProviders(.applicationActivated) }
  await waitUntil { controller.providerRediscoveryTask != nil }
  let unblock = Task { @MainActor in
    await mainActorTurn()
    release.yield()
    release.finish()
  }

  await controller.rediscoverProviders(.userInitiated)
  await activation.value
  await unblock.value

  #expect(recorder.rebuilt == 2)
  #expect(controller.registry.ids == [.codex])
}

@Test @MainActor func reopeningWhileThePopoverIsOpenStartsNoNewAttachment() throws {
  let (controller, _, _) = try startedController()
  defer { controller.stop() }
  let (_, anchor, window) = anchoredPopover()
  defer { window.orderOut(nil) }
  controller.popover?.show(relativeTo: anchor, anchorFrame: nil, visibleFrame: nil)
  try #require(controller.popover?.isShown == true)
  let attachment = controller.launchPopoverTask

  controller.reopen()
  controller.openPopoverForVerification()

  #expect(controller.launchPopoverTask == attachment)
  #expect(controller.popover?.isShown == true)
}
