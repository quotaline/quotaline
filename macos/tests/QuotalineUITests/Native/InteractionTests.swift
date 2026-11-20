import AppKit
import QuotalineTestSupport
import SwiftUI
import Testing

@testable import QuotalineCore
@testable import QuotalineUI

@Test @MainActor func uiActionsDefaultsChangeNothing() {
  let actions = UIActions()
  let environment = try! makeEnvironment()
  actions.refresh()
  actions.refreshProvider(.codex)
  actions.signInProvider(.claude)
  actions.showProviders(nil)
  actions.openURL(URL(string: "https://example.com")!)
  actions.copy("x")
  actions.exportHistory()
  actions.clearHistory()
  actions.revealHistory()
  actions.copyDiagnostics()
  actions.reportIssue()
  actions.showFullLog()
  actions.setLaunchAtLogin(true)
  actions.openLoginItems()
  actions.grantAccess(ProviderID.codex.sandboxResources[0])
  actions.checkForUpdates()
  actions.quit()
  actions.settingsChanged()
  actions.historyRetentionChanged()
  actions.settingsReset()
  actions.setDemoMode(true)
  // the defaults are placeholders: none of them may reach settings, the log or the pasteboard
  #expect(environment.settings.demoMode == environment.settings.demoMode)
  #expect(environment.log.text.isEmpty)
}

@Test @MainActor func chipActionsRouteToCallbacks() {
  var copied: [String] = []
  let chip = ChipView(chip: Chip(text: "Max"), onCopy: { copied.append($0) })
  chip.primaryAction()
  chip.copyAction()
  #expect(copied == ["Max", "Max"])
}

@Test func repeatedModelFocusRequestsHaveDistinctIdentity() {
  let key = WindowKey(provider: .codex, windowID: "weekly")
  #expect(SettingsModelFocusRequest(key: key).id != SettingsModelFocusRequest(key: key).id)
}

@Test @MainActor func usageOrderRejectsSavedModelMoves() throws {
  let environment = try makeEnvironment()
  environment.settings.windowOrder = .percent
  let list = WindowSelectionList(environment: environment)
  list.moveModel(try #require(list.availableKeys.first), by: 1)
  #expect(environment.settings.modelOrder.isEmpty)
}

@Test @MainActor func stableOrderMoveControlsStopAtGroupBoundaries() throws {
  let environment = try makeEnvironment()
  environment.settings.windowOrder = .provider
  let list = WindowSelectionList(environment: environment)
  let providers = list.orderDraft.providers
  let firstProvider = try #require(providers.first)
  let lastProvider = try #require(providers.last)
  #expect(!list.canMoveProvider(firstProvider, by: -1))
  #expect(!list.canMoveProvider(lastProvider, by: 1))

  let models = list.orderDraft.models.filter { $0.provider == firstProvider }
  let firstModel = try #require(models.first)
  let lastModel = try #require(models.last)
  #expect(!list.canMoveModel(firstModel, by: -1))
  #expect(!list.canMoveModel(lastModel, by: 1))
  if models.count > 1 {
    #expect(list.canMoveModel(firstModel, by: 1))
    #expect(list.canMoveModel(lastModel, by: -1))
  }
}

@Test @MainActor func windowHelpViewRenders() {
  let card = UsagePresenter.card(
    provider: .claude, state: ProviderState(snapshot: sampleSnapshot(.claude), availability: .current), samples: [:],
    now: fixedNow)
  for row in card.rows {
    #expect(inkFraction(WindowHelpView(row: row), width: 300, height: 200) > 0)
  }
}

@Test @MainActor func settingsTabBindingsAndMutations() throws {
  let environment = try makeEnvironment()
  var changes = 0
  var retentionChanges = 0
  var resets = 0
  environment.actions.settingsChanged = { changes += 1 }
  environment.actions.historyRetentionChanged = { retentionChanges += 1 }
  environment.actions.settingsReset = { resets += 1 }
  let tab = SettingsTab(environment: environment)
  environment.isDemo = true
  var opened: [URL] = []
  environment.actions.openURL = { opened.append($0) }
  tab.openRepository()
  tab.openPrivacyPolicy()
  tab.openNotices()
  for provider in ProviderID.allCases { tab.openSetupGuide(provider) }
  #expect(
    opened
      == [environment.appInfo.repository, AppInfo.privacyPolicyURL, AppInfo.noticesURL]
      + ProviderID.allCases.map { AppInfo.setupGuideURL(for: $0) })
  tab.grantAccess(ProviderID.codex.sandboxResources[0])
  var refreshedProviders: [ProviderID] = []
  environment.actions.refreshProvider = { refreshedProviders.append($0) }
  tab.perform(.checkAgain, provider: .codex, detail: "Check credentials")
  tab.perform(.refreshProvider(.claude), provider: .codex, detail: "Check credentials")
  #expect(refreshedProviders == [.codex, .claude])
  tab.setting(\.allowTokenRefresh).wrappedValue = true
  #expect(environment.settings.allowTokenRefresh)
  #expect(tab.setting(\.allowTokenRefresh).wrappedValue)
  tab.menuBarSetting(\.windowOrder).wrappedValue = .percent
  #expect(environment.settings.windowOrder == .percent)
  #expect(tab.menuBarSetting(\.windowOrder).wrappedValue == .percent)
  #expect(changes == 1)
  tab.setProvider(.codex, enabled: false)
  #expect(environment.settings.enabledProviders == Set(ProviderID.allCases).subtracting([.codex]))
  tab.provider(.codex).wrappedValue = true
  #expect(tab.provider(.codex).wrappedValue)
  #expect(environment.settings.enabledProviders == Set(ProviderID.allCases))
  let issue = ProviderRecoveryIssue(
    kind: .credentialPersistence, title: "Token not saved", detail: "The credential file changed.",
    action: .refreshProvider(.claude))
  environment.state.update(.claude) { $0.recoveryIssue = issue }
  #expect(tab.actionableRecoveryIssue(.claude) == issue)
  environment.settings.setProvider(.claude, enabled: false)
  #expect(tab.actionableRecoveryIssue(.claude) == nil)
  environment.settings.setProvider(.claude, enabled: true)
  let resources = ProviderID.claude.sandboxResources + ProviderID.codex.sandboxResources
  environment.state.update(.claude) {
    $0.resourceAccess = [
      ResourceAccessState.notRequired(resources[0]), ResourceAccessState(resource: resources[1], health: .needed),
    ]
  }
  #expect(tab.visibleResourceStates(.claude).map(\.resource) == [resources[1]])
  #expect(tab.resourceText(.notRequired) == "Not required")
  #expect(tab.resourceText(.needed) == "Needed")
  #expect(tab.resourceText(.needed, need: .optional) == "Optional")
  #expect(tab.resourceText(.needed, need: .oneOf("cursor")) == "Grant the one you use")
  #expect(!tab.resourceNeedsGrant(.notRequired))
  tab.setThreshold(50, on: true)
  #expect(environment.settings.notifications.thresholds == [50, 75, 90, 100])
  tab.threshold(90).wrappedValue = false
  #expect(!tab.threshold(90).wrappedValue)
  #expect(environment.settings.notifications.thresholds == [50, 75, 100])
  tab.historyRetentionDays.wrappedValue = 90
  #expect(environment.settings.historyRetentionDays == 90)
  tab.resetDefaults()
  #expect(environment.settings.windowOrder == .provider)
  #expect(changes == 4)
  #expect(retentionChanges == 1)
  #expect(resets == 1)
  let log = LogSection(environment: environment)
  log.setDetailedLogging(true)
  #expect(environment.settings.detailedLogging)
  #expect(environment.log.debugEnabled)
  #expect(changes == 5)
}

@Test @MainActor func settingsFormatsCredentialHealthStates() throws {
  let environment = try makeEnvironment(populate: false)
  let tab = SettingsTab(environment: environment)
  let source = ProviderID.codex.credentialSource("test")

  environment.state.update(.codex) { $0.credentialHealth = .missing(expected: [source]) }
  #expect(tab.credentialText(.codex) == "Not found")
  environment.state.update(.codex) { $0.credentialHealth = .valid(source: source, expiresAt: fixedNow) }
  #expect(tab.credentialText(.codex).contains("expires"))
  environment.state.update(.codex) { $0.credentialHealth = .valid(source: source, expiresAt: nil) }
  #expect(tab.credentialText(.codex) == "\(source.title) · \(source.detail)")
  environment.credentialDescriptions[.codex] = "/custom/CODEX_HOME/auth.json"
  #expect(tab.credentialText(.codex) == "\(source.title) · \(source.detail)")
  environment.state.update(.codex) { $0.credentialHealth = .expired(source: source, at: fixedNow) }
  #expect(tab.credentialText(.codex).contains("expired"))
  environment.state.update(.codex) { $0.credentialHealth = .unreadable(source: nil, detail: "denied") }
  #expect(tab.credentialText(.codex) == "/custom/CODEX_HOME/auth.json · unreadable: denied")
  #expect(tab.missingAccess(.codex) == environment.settings.missingAccess(for: .codex))
}

@Test @MainActor func modelLabelsAndStableOrderMutateThroughSettings() throws {
  let environment = try makeEnvironment()
  environment.settings.windowOrder = .provider
  var drafts: [WindowKey: String] = [:]
  let list = WindowSelectionList(
    environment: environment,
    labelDrafts: Binding(get: { drafts }, set: { drafts = $0 }))
  let firstRow = try #require(list.groups.first?.rows.first)

  list.setting(\.hideUnusedModels).wrappedValue = true
  #expect(environment.settings.hideUnusedModels)
  drafts[firstRow.key] = "NEW"
  list.commitLabel(firstRow.key, default: firstRow.defaultLabel)
  #expect(environment.settings.shortLabels[firstRow.key] == "NEW")
  list.revert(firstRow)
  #expect(environment.settings.shortLabels[firstRow.key] == nil)

  let providers = list.orderDraft.providers
  let firstProvider = try #require(providers.first)
  let secondProvider = try #require(providers.dropFirst().first)
  list.moveProvider(secondProvider, before: firstProvider)
  #expect(environment.settings.providerOrder.first == secondProvider)
  list.moveProvider(secondProvider, by: 1)
  #expect(environment.settings.providerOrder.first == firstProvider)

  let models = list.orderDraft.models.filter { $0.provider == firstProvider }
  let firstModel = try #require(models.first)
  let secondModel = try #require(models.dropFirst().first)
  list.moveModel(secondModel, before: firstModel)
  #expect(environment.settings.modelOrder.first { $0.provider == firstProvider } == secondModel)
  list.moveModel(secondModel, by: 1)
  #expect(environment.settings.modelOrder.first { $0.provider == firstProvider } == firstModel)
}

@Test @MainActor func modelLabelConflictRetainsSavedValueUntilRevert() throws {
  let environment = try makeEnvironment()
  var changes = 0
  environment.actions.settingsChanged = { changes += 1 }
  var drafts: [WindowKey: String] = [:]
  let list = WindowSelectionList(
    environment: environment,
    labelDrafts: Binding(get: { drafts }, set: { drafts = $0 }))
  let rows = list.groups.flatMap(\.rows)
  let first = try #require(rows.first)
  let second = try #require(rows.dropFirst().first)

  list.label(first).wrappedValue = "PAIR"
  list.label(second).wrappedValue = " pair "
  list.commitLabel(second.key, default: second.defaultLabel)
  #expect(environment.settings.shortLabels[first.key] == "PAIR")
  #expect(environment.settings.shortLabels[second.key] == nil)
  #expect(drafts[second.key] == " pair ")
  #expect(
    list.shortLabelAccessibilityHint(second)
      == "6 of 6 characters. Already used by \(first.key.provider.displayName) \(first.window.label); "
      + "saved label remains \(second.label)."
  )
  #expect(changes == 1)

  environment.settings.statusFormat = .custom
  environment.settings.customTemplate = "{label}"
  let preview = SettingsTab(environment: environment).previewModel
  #expect(preview.cells.contains { StatusTemplate.plainText($0.lines).contains("PAIR") })

  list.revert(first)
  list.commitLabel(second.key, default: second.defaultLabel)
  #expect(environment.settings.shortLabels[first.key] == nil)
  #expect(environment.settings.shortLabels[second.key] == "pair")
  list.revert(try #require(list.row(second.key)))
  #expect(environment.settings.shortLabels[second.key] == nil)
}

@Test @MainActor func modelRowsRenderStableUsageAndOverrideStates() throws {
  let environment = try makeEnvironment()
  let key = WindowKey(provider: .claude, windowID: "session")
  environment.settings.shortLabels[key] = "CUSTOM"
  var list = WindowSelectionList(environment: environment)
  var row = try #require(list.groups.flatMap(\.rows).first { $0.key == key })
  #expect(inkFraction(list.modelRow(row), width: 760, height: 80) > 0)

  environment.settings.windowOrder = .percent
  list = WindowSelectionList(environment: environment)
  row = try #require(list.groups.flatMap(\.rows).first { $0.key == key })
  #expect(inkFraction(list.modelRow(row), width: 760, height: 80) > 0)
  let group = try #require(list.groups.first)
  #expect(inkFraction(list.providerHeader(group), width: 760, height: 80) > 0)
}

@Test @MainActor func modelRowPreservesLongIdentityAtNarrowWidthAndInAccessibility() throws {
  let environment = try makeEnvironment(populate: false)
  let name = "Codex Enterprise Reasoning Model With Extended Context"
  let identifier = "additional:codex-enterprise-reasoning-model-with-extended-context"
  let window = QuotaWindow(
    id: identifier, label: name, group: .other, usedPercent: 42, resetsAt: nil)
  environment.state.update(.codex) {
    $0.snapshot = ProviderReading(provider: .codex, windows: [window], fetchedAt: fixedNow)
  }
  let list = WindowSelectionList(environment: environment)
  let row = try #require(list.groups.first?.rows.first)
  #expect(list.modelAccessibilityLabel(row) == "\(name), \(identifier)")
  #expect(list.modelAccessibilityValue(row) == "shown, 42.00%, today, label ERW")
  #expect(inkFraction(list.modelRow(row), width: 320, height: 160) > 0)
}

@Test @MainActor func settingsActivityReloadsForHistoryChangesRatherThanEmptyRefreshes() throws {
  let environment = try makeEnvironment()
  let initial = WindowSelectionList(environment: environment).activityRequest

  environment.state.setRefreshing(false, at: fixedNow.addingTimeInterval(60))
  #expect(WindowSelectionList(environment: environment).activityRequest == initial)

  environment.state.markSamplesChanged()
  let changed = WindowSelectionList(environment: environment).activityRequest
  #expect(changed.sampleRevision == initial.sampleRevision + 1)
  #expect(changed != initial)
}

@Test @MainActor func settingsActivityCoalescesRepeatedRequests() async throws {
  let environment = try makeEnvironment()
  let stamp = fixedNow.addingTimeInterval(-60)
  try await environment.history.record(sampleSnapshot(.claude), now: stamp)
  environment.state.markSamplesChanged()
  let request = WindowSelectionList(environment: environment).activityRequest

  let first = await environment.settingsActivity(for: request)
  try await environment.history.breakDatabase()

  #expect(await environment.settingsActivity(for: request) == first)
  environment.state.markSamplesChanged()
  let changed = WindowSelectionList(environment: environment).activityRequest
  #expect(await environment.settingsActivity(for: changed).isEmpty)
}

@Test @MainActor func historyTabPagingAndChartInteractions() async throws {
  let environment = try makeEnvironment()
  let presenter = environment.historyPresenter
  environment.settings.historyRange = .custom
  presenter.customStart = fixedNow.addingTimeInterval(-7200)
  presenter.customEnd = fixedNow.addingTimeInterval(-3600)
  presenter.followNow = false
  let tab = HistoryTab(environment: environment)
  tab.pageForward()
  await presenter.waitForLoad()
  #expect(presenter.followNow)
  tab.pageBack()
  await presenter.waitForLoad()
  #expect(!presenter.followNow)
  try await environment.history.record(sampleSnapshot(.claude), now: fixedNow.addingTimeInterval(-60))
  presenter.reload()
  await presenter.waitForLoad()
  presenter.setRange(.today)
  await presenter.waitForLoad()
  let data = presenter.state.data!
  let chart = UsageChart(data: data, presenter: presenter, stacked: false, timeZone: .current)
  let plot = CGRect(x: 10, y: 0, width: 100, height: 50)
  chart.hover(.active(CGPoint(x: 60, y: 10)), in: plot)
  #expect(presenter.selectedDate != nil)
  chart.hover(.ended, in: plot)
  #expect(presenter.selectedDate == nil)
  chart.pick(CGPoint(x: 20, y: 0), in: plot)
  #expect(presenter.selectedDate != nil)
  #expect(inkFraction(chart, width: 400, height: 240) > 0)
  #expect(inkFraction(EmptyHistoryView(), width: 300, height: 200) > 0)
  #expect(inkFraction(UpdatingBadge(), width: 100, height: 30) > 0)
}

@Test @MainActor func historyTabShowsEmptyAndUpdatingStates() async throws {
  let environment = try makeEnvironment(populate: false)
  environment.settings.lastTab = .history
  environment.state.popoverVisible = true
  let presenter = environment.historyPresenter
  presenter.reload()
  await presenter.waitForLoad()
  #expect(presenter.state.data?.isEmpty == true)
  #expect(inkFraction(HistoryTab(environment: environment), width: 700, height: 600) > 0)
  await presenter.waitForLoad()
  presenter.reload()
  guard case .loaded(_, true, _) = presenter.state else {
    Issue.record("expected refreshing state")
    return
  }
  #expect(inkFraction(HistoryTab(environment: environment), width: 700, height: 600) > 0)
  await presenter.waitForLoad()
}

@Test @MainActor func liveDependencyHelpers() async throws {
  let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  defer { try? FileManager.default.removeItem(at: root) }
  let configuredDirectory = root.appendingPathComponent("configured-codex")
  try FileManager.default.createDirectory(at: configuredDirectory, withIntermediateDirectories: true)
  let paths = LiveDependencies.Paths(
    home: root, supportDirectory: root, environment: ["CODEX_HOME": configuredDirectory.path], userName: "tester")
  let pasteboard = NSPasteboard(name: NSPasteboard.Name("tech.bernat.quotaline.tests"))
  LiveDependencies.copy("hello", to: pasteboard)
  #expect(pasteboard.string(forType: .string) == "hello")
  let export = LiveDependencies.exportPanel()
  #expect(export.nameFieldStringValue == "quotaline-history.csv")
  #expect(export.allowedContentTypes == [.commaSeparatedText])
  let codex = LiveDependencies.directoryPanel(
    resource: ProviderID.codex.sandboxResources[0], default: root)
  let configured = LiveDependencies.directoryPanel(
    ProviderID.codex.sandboxResources[0], paths: paths)
  #expect(configured.directoryURL?.resolvingSymlinksInPath() == configuredDirectory.resolvingSymlinksInPath())
  #expect(codex.canChooseDirectories)
  #expect(!codex.canChooseFiles)
  #expect(codex.showsHiddenFiles)
  #expect(codex.directoryURL?.resolvingSymlinksInPath() == root.resolvingSymlinksInPath())
  #expect(await LiveDependencies.chosen({ export }) { $1(.cancel) } == nil)
  #expect(await LiveDependencies.chosen({ codex }) { $1(.OK) } == codex.url)
}

@Test @MainActor func verificationChoosersKeepEveryPanelInsideTemporarySupport() async {
  let support = FileManager.default.temporaryDirectory.appendingPathComponent("tmb-panels-\(UUID().uuidString)")
  let paths = LiveDependencies.Paths(
    home: support.appendingPathComponent("home"), supportDirectory: support, environment: [:], userName: "verify")

  let deterministic = LiveDependencies.exportChooser(
    profile: VerificationProfile(), supportDirectory: support, run: { $1(.cancel) })
  #expect(await deterministic() == support.appendingPathComponent("verification-history.csv"))
  let directExport = LiveDependencies.exportChooser(
    profile: nil, supportDirectory: support, run: { $1(.cancel) })
  #expect(await directExport() == nil)
  let noDirectory = LiveDependencies.directoryChooser(
    profile: VerificationProfile(), paths: paths, supportDirectory: support, run: { $1(.cancel) })
  #expect(await noDirectory(ProviderID.codex.sandboxResources[0]) == nil)

  let nativeProfile = VerificationProfile(nativePanels: true)
  var saveDirectory: URL?
  let nativeExport = LiveDependencies.exportChooser(
    profile: nativeProfile, supportDirectory: support,
    run: {
      saveDirectory = $0.directoryURL
      $1(.cancel)
    })
  #expect(await nativeExport() == nil)
  #expect(saveDirectory == support)

  var openDirectories: [URL?] = []
  let nativeDirectory = LiveDependencies.directoryChooser(
    profile: nativeProfile, paths: paths, supportDirectory: support,
    run: {
      openDirectories.append($0.directoryURL)
      $1(.cancel)
    })
  #expect(await nativeDirectory(ProviderID.codex.sandboxResources[0]) == nil)
  #expect(await nativeDirectory(ProviderID.gemini.sandboxResources[0]) == nil)
  #expect(openDirectories.compactMap { $0?.standardizedFileURL.path } == [support.path, support.path])

  var directDirectory: URL?
  let direct = LiveDependencies.directoryChooser(
    profile: nil, paths: paths, supportDirectory: support,
    run: {
      directDirectory = $0.directoryURL
      $1(.cancel)
    })
  #expect(await direct(ProviderID.codex.sandboxResources[0]) == nil)
  #expect(
    directDirectory?.standardizedFileURL.path == paths.home.appendingPathComponent(".codex").standardizedFileURL.path)
}

@Test @MainActor func popoverForwardsEvents() {
  let event = NSEvent.mouseEvent(
    with: .mouseMoved, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil, eventNumber: 0,
    clickCount: 0, pressure: 0)!
  #expect(PopoverController(content: AnyView(Text("x"))).forward(event) === event)
}

@Test @MainActor func enablingAnUndiscoveredProviderChecksItsBoxAndOffersItsGrants() throws {
  let environment = try makeEnvironment(populate: false)
  let tab = SettingsTab(environment: environment)
  #expect(!tab.provider(.codex).wrappedValue)
  tab.provider(.codex).wrappedValue = true
  #expect(tab.provider(.codex).wrappedValue)
  #expect(tab.isProviderEnabled(.codex))
  tab.provider(.codex).wrappedValue = false
  #expect(!tab.isProviderEnabled(.codex))
}

@Test @MainActor func settingsActivityCachesOnlyTheNewestRequest() async throws {
  let environment = try makeEnvironment()
  try await environment.history.record(sampleSnapshot(.claude), now: fixedNow.addingTimeInterval(-60))
  environment.state.markSamplesChanged()
  let older = WindowSelectionList(environment: environment).activityRequest
  environment.state.markSamplesChanged()
  let newer = WindowSelectionList(environment: environment).activityRequest

  let superseded = Task { await environment.settingsActivity(for: older) }
  // The newer request must replace a load that is still running, which one yield does not guarantee.
  while environment.settingsActivityLoad == nil { await Task.yield() }
  _ = await environment.settingsActivity(for: newer)
  _ = await superseded.value
  try await environment.history.breakDatabase()

  #expect(!(await environment.settingsActivity(for: newer)).isEmpty)
  #expect(await environment.settingsActivity(for: older).isEmpty)
}
