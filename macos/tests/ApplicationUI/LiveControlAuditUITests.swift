import AppKit
import CoreGraphics
import Foundation
import QuotalineCore
import XCTest

final class LiveControlAuditUITests: XCTestCase {
  let controlTimeout = 5.0
  // A provider's All checkbox changes several rows at once and re-renders the whole model list, which the 1024x768
  // runners settle in about four seconds when idle and in more than five when they are busy.
  let groupChangeTimeout = 20.0

  override func tearDownWithError() throws {
    let directory = try outputDirectory()
    for file in try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
    where ["png", "json"].contains(file.pathExtension) {
      let attachment = XCTAttachment(
        data: try Data(contentsOf: file),
        uniformTypeIdentifier: file.pathExtension == "png" ? "public.png" : "public.json")
      attachment.name = file.lastPathComponent
      attachment.lifetime = .keepAlways
      add(attachment)
    }
    try super.tearDownWithError()
  }

  @MainActor
  func testUsageControlsRespond() throws {
    try auditControls(tab: "Usage")
  }

  @MainActor
  func testHistoryDatesAndControlsRespond() throws {
    try auditControls(tab: "History")
  }

  @MainActor
  func testHistoryExportSavesTheSelectedPeriod() throws {
    let verification = VerificationApplication(
      testName: name, profile: VerificationProfile(fixture: .controlAudit, nativePanels: true))
    addTeardownBlock { @MainActor in await verification.terminate() }
    try verification.launch()
    XCTAssertTrue(verification.tabs.waitUntilExists(timeout: 5))
    verification.tab("History").click()
    let application = verification.application
    let period = application.descendants(matching: .any)["history-period"]
    segment("Today", in: period).click()
    let destination = verification.supportDirectory.appendingPathComponent("quotaline-history.csv")
    XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))

    application.buttons["history-export"].click()
    let save = application.sheets["save-panel"].buttons["OKButton"]
    XCTAssertTrue(save.waitUntilExists(timeout: 2))
    XCTAssertTrue(save.isHittable)
    save.click()

    XCTAssertTrue(
      waitUntil(timeout: 2) {
        ((try? FileManager.default.attributesOfItem(atPath: destination.path)[.size] as? NSNumber)?.intValue ?? 0)
          > "timestamp,key,label,used_percent,resets_at\n".utf8.count
      }, "The selected-period export did not write its header and synthetic samples")
    let rows = try String(contentsOf: destination, encoding: .utf8).split(separator: "\n")
    XCTAssertEqual(rows.first, "timestamp,key,label,used_percent,resets_at")
    XCTAssertGreaterThan(rows.count, 1, "The selected period exported no synthetic samples")
    XCTAssertTrue(verification.tabs.isEnabled)
    verification.tab("Usage").click()
    XCTAssertTrue(application.descendants(matching: .any)["tab-content-Usage"].waitUntilExists(timeout: 2))
  }

  @MainActor
  func testSettingsMenuBarControlsRespond() throws {
    try auditControls(tab: "Settings", section: .menuBar)
  }

  @MainActor func testSettingsModelSelectionControlsRespond() throws {
    try auditControls(tab: "Settings", section: .menuBar, modelSelectionOnly: true)
  }

  @MainActor func testSettingsAboutControlsRespond() throws {
    try auditControls(tab: "Settings", section: .about)
  }

  @MainActor func testSettingsProviderControlsRespond() throws {
    try auditControls(tab: "Settings", section: .providers)
  }

  @MainActor func testSettingsProviderAccessActionsRespond() throws {
    executionTimeAllowance = 420
    let verification = VerificationApplication(
      testName: name, profile: VerificationProfile(fixture: .controlAudit, nativePanels: true), detailedLogging: true)
    addTeardownBlock { @MainActor in await verification.terminate() }
    try verification.launch()
    XCTAssertTrue(verification.tabs.waitUntilExists(timeout: 5))
    verification.tab("Settings").click()
    let application = verification.application
    let surface = application.descendants(matching: .any)["popover-surface"]
    let records = exerciseProviderAccessActions(
      application, surface: surface, supportDirectory: verification.supportDirectory)
    XCTAssertTrue(verification.tabs.isEnabled, "A cancelled file chooser left the popover tabs disabled")
    let output = try outputDirectory()
    try write(records, to: output.appendingPathComponent("control-matrix-provider-access.json"))
    try application.screenshot().pngRepresentation.write(to: output.appendingPathComponent("provider-access.png"))
    XCTAssertTrue(records.allSatisfy { $0.result == "passed" })
    verification.tab("Usage").click()
    XCTAssertTrue(application.descendants(matching: .any)["tab-content-Usage"].waitUntilExists(timeout: 2))
    print("CONTROL_AUDIT=\(output.path) controls=\(records.count)")
  }

  @MainActor func testSettingsDataControlsRespond() throws {
    try auditControls(tab: "Settings", section: .data)
  }

  @MainActor func testSettingsNotificationControlsRespond() throws {
    try auditControls(tab: "Settings", section: .notifications)
  }

  @MainActor func testSettingsLogControlsRespond() throws {
    try auditControls(tab: "Settings", section: .log)
  }

  @MainActor
  private func auditControls(
    tab: String, section: SettingsSection? = nil, modelSelectionOnly: Bool = false
  ) throws {
    executionTimeAllowance = 600
    let verification = VerificationApplication(
      testName: name, profile: VerificationProfile(fixture: .controlAudit, nativePanels: true),
      detailedLogging: tab == "History" || section == .menuBar)
    addTeardownBlock { @MainActor in await verification.terminate() }
    try verification.launch()
    XCTAssertTrue(verification.tabs.waitUntilExists(timeout: 5))
    verification.bringToFront()

    let output = try outputDirectory()
    var records: [ControlAuditRecord] = []
    defer { try? write(records, to: output.appendingPathComponent("control-matrix-\(tab.lowercased()).json")) }
    do {
      if tab != "Usage" { verification.tab(tab).click() }
      XCTAssertTrue(
        verification.application.descendants(matching: .any)["tab-content-\(tab)"].waitUntilExists(timeout: 2))
      if tab == "Settings" {
        XCTAssertTrue(verification.application.textFields["model-filter"].waitUntilExists(timeout: 2))
      }
      if tab != "History" && !modelSelectionOnly {
        try expandSupportingDetails(in: verification.application, section: section)
      }
      if tab == "History" {
        records += try exerciseHistoryControls(
          verification.application, supportDirectory: verification.supportDirectory)
        XCTAssertTrue(verification.tabs.isEnabled, "The cancelled export left the popover tabs disabled")
      }
      if tab == "Settings" {
        let application = verification.application
        let surface = application.descendants(matching: .any)["popover-surface"]
        switch section! {
        case .about:
          records += exerciseAboutControls(application, surface: surface)
          records += try exerciseResetAndReopen(
            application, statusItem: verification.statusItem,
            processIdentifier: verification.processIdentifier(), reopen: verification.openPopover)
        case .menuBar:
          records +=
            try modelSelectionOnly
            ? exerciseModelSelectionControls(application)
            : exerciseMenuBarControls(application, statusItem: verification.statusItem)
        case .providers:
          records += exerciseProviderControls(application, surface: surface)
        case .data:
          records += exerciseDataControls(
            application, surface: surface, supportDirectory: verification.supportDirectory)
        case .notifications:
          records += exerciseNotificationControls(application, surface: surface)
        case .log:
          records += exerciseLogControls(application, surface: surface, reopen: verification.openPopover)
        }
      }
      if tab == "Usage" { records += exerciseUsageControls(verification.application) }
      try verification.application.screenshot().pngRepresentation.write(
        to: output.appendingPathComponent("controls-\(tab.lowercased()).png"))
    }

    let failures = records.filter { $0.result.hasPrefix("failed") }
    assertRequiredInventory(records, tab: tab, section: section, modelSelectionOnly: modelSelectionOnly)
    XCTAssertTrue(failures.isEmpty, failures.map { "\($0.tab): \($0.label) \($0.result)" }.joined(separator: "\n"))
    XCTAssertTrue(records.contains { $0.tab == tab && $0.interacted })
    print("CONTROL_AUDIT=\(output.path) controls=\(records.count) failures=\(failures.count)")
  }

  @MainActor
  func testLongTextAtWideWidthInLightAppearance() throws {
    try auditLongText(appearance: .light, widthName: "wide", width: nil)
  }

  @MainActor
  func testLongTextAtNarrowWidthInLightAppearance() throws {
    try auditLongText(appearance: .light, widthName: "narrow", width: 752)
  }

  @MainActor
  func testLongTextAtWideWidthInDarkAppearance() throws {
    try auditLongText(appearance: .dark, widthName: "wide", width: nil)
  }

  @MainActor
  func testLongTextAtNarrowWidthInDarkAppearance() throws {
    try auditLongText(appearance: .dark, widthName: "narrow", width: 752)
  }

  /// Audits every tab in one launch. Each phase restores what it changes: steppers return to their values, and the
  /// pointer leaves the panel so no tooltip from one tab covers the next one's first page.
  @MainActor
  private func auditLongText(appearance: VerificationApplication.Appearance, widthName: String, width: Double?) throws {
    executionTimeAllowance = 600
    let output = try outputDirectory()
    let verification = VerificationApplication(
      testName: "\(name)-\(appearance.rawValue)-\(widthName)",
      profile: VerificationProfile(fixture: .longText, visibleFrameWidth: width),
      appearance: appearance, doubleLocalizedStrings: true, detailedLogging: true)
    addTeardownBlock { @MainActor in await verification.terminate() }
    try verification.launch()
    XCTAssertTrue(verification.tabs.waitUntilExists(timeout: 5))
    let surface = verification.application.descendants(matching: .any)["popover-surface"]
    XCTAssertTrue(surface.waitUntilExists(timeout: 2))
    let window = verification.application.windows["Quotaline"]
    let chromeWidth = window.frame.width - surface.frame.width
    let expectedWidth = min(880, (width ?? Double.greatestFiniteMagnitude) - 24 - chromeWidth)

    for tab in ["Usage", "History", "Settings"] {
      movePointerOffPanel(in: verification.application)
      verification.tab(tab).click()
      XCTAssertTrue(
        verification.application.descendants(matching: .any)["tab-content-\(tab)"].waitUntilExists(timeout: 2))
      XCTAssertEqual(surface.frame.width, expectedWidth, accuracy: 3, "\(tab) opened at another width")
      if tab == "Settings" {
        XCTAssertTrue(verification.application.textFields["model-filter"].waitUntilExists(timeout: 2))
      }
      if tab != "History" { try expandSupportingDetails(in: verification.application) }
      if tab == "Usage" {
        XCTAssertTrue(
          verification.application.descendants(matching: .any).matching(
            NSPredicate(
              format: "label CONTAINS %@", "Verification warning text is intentionally long")
          ).firstMatch.waitUntilExists(timeout: 5))
      }
      let exposedText = try captureAndAuditPages(
        tab: tab, surface: surface, statusItem: verification.statusItem, application: verification.application,
        output: output,
        prefix: "\(appearance.rawValue.lowercased())-\(widthName)")
      if tab == "Usage" {
        XCTAssertTrue(exposedText.contains { $0.contains("Verification warning text is intentionally long") })
        XCTAssertTrue(exposedText.contains { $0.contains("usage window with a deliberately long model name") })
      }
      if tab == "Settings" {
        XCTAssertTrue(exposedText.contains { $0.contains("account-profile-with-a-deliberately-long-file-name") })
      }
    }
    print("LONG_TEXT_AUDIT=\(output.path)")
  }

  /// Audits every tab in one launch. Each phase scrolls its tab back by the amount it scrolled and ends with the
  /// pointer off the panel and no tooltip on screen, so the next phase starts from the state a fresh launch shows.
  @MainActor
  func testRichTooltipsStayAdjacentAndTabsHaveNone() throws {
    executionTimeAllowance = 270
    let verification = VerificationApplication(testName: name, detailedLogging: true)
    addTeardownBlock { @MainActor in await verification.terminate() }
    try verification.launch()
    XCTAssertTrue(verification.tabs.waitUntilExists(timeout: 5))
    let output = try outputDirectory()
    let processIdentifier = try verification.processIdentifier()
    let surface = verification.application.descendants(matching: .any)["popover-surface"]

    for tab in ["Usage", "History", "Settings"] {
      let baseline = applicationWindows(processIdentifier: processIdentifier)
      try auditRichTooltips(tab: tab, verification: verification, processIdentifier: processIdentifier, output: output)
      movePointerOffPanel(in: verification.application)
      XCTAssertTrue(
        waitUntil(timeout: 0.25) {
          applicationWindows(processIdentifier: processIdentifier).allSatisfy { baseline[$0.key] != nil }
        }, "\(tab) left a tooltip on screen")
    }

    for tab in ["Usage", "History", "Settings"] {
      movePointerOffPanel(in: verification.application)
      Thread.sleep(forTimeInterval: 0.2)
      let baseline = applicationWindows(processIdentifier: processIdentifier)
      verification.tab(tab).hover()
      Thread.sleep(forTimeInterval: 0.25)
      let newWindows = applicationWindows(processIdentifier: processIdentifier).filter { baseline[$0.key] == nil }
      XCTAssertTrue(newWindows.isEmpty, "Tab \(tab) presented a tooltip")
    }

    verification.tab("Settings").click()
    if let control = try tooltipCandidates(in: surface).first?.1 {
      movePointerOffPanel(in: verification.application)
      let tabBaseline = applicationWindows(processIdentifier: processIdentifier)
      control.hover()
      let tabTooltip = try waitForTooltip(
        processIdentifier: processIdentifier, excluding: tabBaseline, timeout: 0.75)
      verification.tab("Usage").click()
      XCTAssertTrue(
        waitUntil(timeout: 0.15) {
          applicationWindows(processIdentifier: processIdentifier)[tabTooltip.identifier] == nil
        }, "Tab switching did not dismiss the tooltip immediately")

      let escapeBaseline = applicationWindows(processIdentifier: processIdentifier)
      try tooltipCandidates(in: surface).first?.1.hover()
      let escapeTooltip = try waitForTooltip(
        processIdentifier: processIdentifier, excluding: escapeBaseline, timeout: 0.75)
      verification.application.typeKey(.escape, modifierFlags: [])
      XCTAssertTrue(
        waitUntil(timeout: 0.15) {
          applicationWindows(processIdentifier: processIdentifier)[escapeTooltip.identifier] == nil
        }, "Escape did not dismiss the tooltip immediately")
      XCTAssertTrue(verification.tabs.waitForNonExistence(timeout: 2), "Escape did not close the popup")
      XCTAssertTrue(verification.reopenPopover())
    }
    print("TOOLTIP_AUDIT=\(output.path)")
  }

  @MainActor
  private func auditRichTooltips(
    tab: String, verification: VerificationApplication, processIdentifier: pid_t, output: URL
  ) throws {
    verification.tab(tab).click()
    XCTAssertTrue(
      verification.application.descendants(matching: .any)["tab-content-\(tab)"].waitUntilExists(timeout: 2))
    if tab == "Settings" {
      XCTAssertTrue(verification.application.textFields["model-filter"].waitUntilExists(timeout: 2))
    }
    let surface = verification.application.descendants(matching: .any)["popover-surface"]
    XCTAssertGreaterThanOrEqual(
      try tooltipCandidates(in: surface).count, 5, "\(tab) did not expose tooltip controls across the panel")
    for position in ["top", "bottom", "left", "right", "center"] {
      let control = try XCTUnwrap(tooltipCandidates(in: surface).first { $0.0 == position }?.1)
      XCTAssertTrue(
        waitUntil(timeout: controlTimeout) { control.isHittable },
        "Tooltip target \(control.identifier) is not hittable at \(control.frame)")
      print("TOOLTIP_TARGET tab=\(tab) position=\(position) id=\(control.identifier) frame=\(control.frame)")
      let delay: TimeInterval = tab == "History" && !control.identifier.hasPrefix("footer-") ? 1.2 : 0.15
      movePointerOffPanel(in: verification.application)
      Thread.sleep(forTimeInterval: 0.15)
      let baseline = applicationWindows(processIdentifier: processIdentifier)
      let hoverStarted = ProcessInfo.processInfo.systemUptime
      control.hover()
      let hoverReturned = ProcessInfo.processInfo.systemUptime
      if hoverReturned - hoverStarted < delay - 0.05 {
        Thread.sleep(forTimeInterval: max(0, delay - 0.02 - (ProcessInfo.processInfo.systemUptime - hoverStarted)))
        XCTAssertTrue(
          applicationWindows(processIdentifier: processIdentifier).allSatisfy { baseline[$0.key] != nil },
          "Tooltip appeared before its \(Int(delay * 1_000)) ms threshold")
      }
      let tooltip = try waitForTooltip(
        processIdentifier: processIdentifier, excluding: baseline, timeout: delay + 0.6)
      let showLatency = ProcessInfo.processInfo.systemUptime - hoverStarted
      XCTAssertGreaterThanOrEqual(showLatency, delay - 0.02, "Tooltip appeared before its configured delay")
      let delta = distance(between: control.frame, and: tooltip.frame)
      XCTAssertLessThanOrEqual(delta, 20, "Tooltip was not adjacent to \(control.label)")
      XCTAssertTrue(CGDisplayBounds(CGMainDisplayID()).contains(tooltip.frame), "Tooltip left the active screen")
      control.coordinate(withNormalizedOffset: CGVector(dx: 0.75, dy: 0.5)).hover()
      Thread.sleep(forTimeInterval: 0.05)
      XCTAssertNotNil(
        applicationWindows(processIdentifier: processIdentifier)[tooltip.identifier],
        "Tooltip flickered while the pointer stayed inside \(control.label)")
      try verification.application.screenshot().pngRepresentation.write(
        to: output.appendingPathComponent("tooltip-\(tab.lowercased())-\(position).png"))
      print(
        "TOOLTIP tab=\(tab) control=\(control.label) position=\(position) show_ms=\(Int(showLatency * 1_000)) "
          + "delta=\(delta) frame=\(tooltip.frame)")
      movePointerOffPanel(in: verification.application)
      let dismissStarted = ProcessInfo.processInfo.systemUptime
      XCTAssertTrue(
        waitUntil(timeout: 0.25) {
          applicationWindows(processIdentifier: processIdentifier)[tooltip.identifier] == nil
        }, "Tooltip did not dismiss by the 250 ms scheduler-safe ceiling")
      print(
        "TOOLTIP_DISMISS tab=\(tab) control=\(control.label) after_driver_ms=\(Int((ProcessInfo.processInfo.systemUptime - dismissStarted) * 1_000))"
      )
    }

    guard
      let scrollView = surface.scrollViews.allElementsBoundByIndex.first(where: { $0.isHittable }),
      let control = try tooltipCandidates(in: surface).first?.1
    else { return }
    let baseline = applicationWindows(processIdentifier: processIdentifier)
    control.hover()
    let tooltip = try waitForTooltip(
      processIdentifier: processIdentifier, excluding: baseline, timeout: tab == "History" ? 1.8 : 0.75)
    scrollView.scroll(byDeltaX: 0, deltaY: -180)
    defer { scrollView.scroll(byDeltaX: 0, deltaY: 180) }
    XCTAssertTrue(
      waitUntil(timeout: 0.15) {
        applicationWindows(processIdentifier: processIdentifier)[tooltip.identifier] == nil
      }, "Scrolling did not dismiss the tooltip immediately")
    movePointerOffPanel(in: verification.application)
    XCTAssertTrue(
      waitUntil(timeout: 0.25) {
        applicationWindows(processIdentifier: processIdentifier).allSatisfy { baseline[$0.key] != nil }
      }, "Scrolling left a tooltip on screen")
    let scrolledControl = verification.application.buttons["footer-refresh"]
    XCTAssertTrue(waitUntil(timeout: controlTimeout) { scrolledControl.isHittable })
    let scrolledBaseline = applicationWindows(processIdentifier: processIdentifier)
    scrolledControl.hover()
    let scrolledTooltip = try waitForTooltip(
      processIdentifier: processIdentifier, excluding: scrolledBaseline, timeout: tab == "History" ? 1.8 : 0.75)
    XCTAssertLessThanOrEqual(distance(between: scrolledControl.frame, and: scrolledTooltip.frame), 20)
    XCTAssertTrue(CGDisplayBounds(CGMainDisplayID()).contains(scrolledTooltip.frame))
    try verification.application.screenshot().pngRepresentation.write(
      to: output.appendingPathComponent("tooltip-\(tab.lowercased())-after-scroll.png"))
    movePointerOffPanel(in: verification.application)
    XCTAssertTrue(
      waitUntil(timeout: 0.25) {
        applicationWindows(processIdentifier: processIdentifier)[scrolledTooltip.identifier] == nil
      })
  }
}
