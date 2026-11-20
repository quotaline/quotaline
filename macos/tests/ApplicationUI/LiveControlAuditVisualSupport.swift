import AppKit
import CoreGraphics
import Foundation
import QuotalineCore
import XCTest

extension LiveControlAuditUITests {
  @MainActor
  func assertVisibleStringsAndControls(
    in surface: XCUIElement, application: XCUIApplication, snapshot: any XCUIElementSnapshot,
    auditedControls: inout Set<[String]>
  ) -> Int {
    activate(application)
    let visible = visibleSnapshots(in: snapshot, within: snapshot.frame)
    for text in visible where text.elementType == .staticText {
      XCTAssertFalse(text.label.isEmpty && (text.value as? String ?? "").isEmpty, "Visible text has no accessible text")
    }
    let banners = visible.filter {
      $0.label.hasPrefix("Warning: Verification warning text is intentionally long")
    }
    for banner in banners {
      assertRenderedText(
        element(for: banner, in: surface, tree: snapshot), expected: String(banner.label.dropFirst("Warning: ".count)))
    }
    for control in visible where Self.audits(control.elementType, identifier: control.identifier) && control.isEnabled {
      guard
        auditedControls.insert([
          String(control.elementType.rawValue), control.identifier, control.label, control.title,
          String(describing: control.value), String(describing: control.frame),
        ]).inserted
      else { continue }
      if control.elementType == .stepper { continue }
      let element = element(for: control, in: surface, tree: snapshot)
      // XCTest's hit test can briefly miss a control whose frame has not changed; reveal waits for the same reason.
      let hittable = waitUntil(timeout: controlTimeout) { element.isHittable }
      if !hittable {
        let attachment = XCTAttachment(string: application.debugDescription)
        attachment.name = "Inaccessible visible control"
        attachment.lifetime = .keepAlways
        add(attachment)
      }
      XCTAssertTrue(
        hittable,
        "Visible enabled control is not hittable: \(control.elementType) \(control.identifier) "
          + "\(control.label) \(control.frame), now at \(element.frame)")
    }
    XCTAssertEqual(application.state, .runningForeground)
    return banners.count
  }

  @MainActor
  func assertRenderedText(_ element: XCUIElement, expected: String) {
    do {
      try RenderedText(suffix: expected, layout: .paragraph).capture(
        element.screenshot().pngRepresentation,
        at: outputDirectory().appendingPathComponent("banner-\(UUID().uuidString).png"))
    } catch {
      XCTFail("Could not capture rendered-text assertion: \(error)")
    }
  }

  @MainActor
  func expandSupportingDetails(in application: XCUIApplication, section: SettingsSection? = nil) throws {
    let surface = application.descendants(matching: .any)["popover-surface"]
    let snapshot = try surface.snapshot()
    let elements = snapshots(in: snapshot)
    if let section {
      XCTAssertTrue(
        elements.contains { $0.identifier == "settings-section-\(section.rawValue)" },
        "Missing Settings section \(section.rawValue)")
    }
    var expanded: Set<String> = []
    for heading in elements.sorted(by: { $0.frame.minY < $1.frame.minY })
    where heading.identifier.hasPrefix("disclosure-") && !heading.identifier.hasPrefix("disclosure-content-") {
      let identifier = heading.identifier
      guard expanded.insert(identifier).inserted else { continue }
      if let section, !disclosure(identifier, belongsTo: section) { continue }
      let content = application.descendants(matching: .any)[
        "disclosure-content-" + identifier.dropFirst("disclosure-".count)]
      if content.exists { continue }
      let group = surface.descendants(matching: .any).matching(identifier: identifier).firstMatch
      let disclosure = group.elementType == .disclosureTriangle ? group : group.disclosureTriangles.firstMatch
      guard disclosure.exists else {
        XCTFail("Missing native disclosure for \(identifier)")
        continue
      }
      XCTAssertTrue(
        reveal(disclosure, in: surface),
        "Cannot reach \(identifier): control=\(disclosure.frame) scroll=\(surface.scrollViews.firstMatch.frame) "
          + "surface=\(surface.frame)")
      disclosure.click()
      XCTAssertTrue(
        content.waitUntilExists(timeout: controlTimeout),
        "\(identifier) did not expand after clicking its heading")
    }
    scrollToTop(surface)
  }

  func disclosure(_ identifier: String, belongsTo section: SettingsSection) -> Bool {
    section == .providers && identifier.hasPrefix("disclosure-connection.")
  }

  @MainActor
  func captureAndAuditPages(
    tab: String, surface: XCUIElement, statusItem: XCUIElement, application: XCUIApplication, output: URL,
    prefix: String
  ) throws -> [String] {
    let scrollView = surface.scrollViews.firstMatch
    let hasScrollView = scrollView.exists
    var frame = surface.frame
    var anchor = statusItem.frame
    let viewport = hasScrollView ? scrollView.frame.intersection(frame) : frame
    let point = coordinateOutsidePanel(surface, frame: frame).screenPoint
    XCTAssertTrue(point.x.isFinite && point.y.isFinite, "Pointer target must have finite screen coordinates")
    var exposedText: [String] = []
    var previousPage: Set<String> = []
    var checkedBanners = 0
    var auditedControls: Set<[String]> = []
    var adjustedSteppers: Set<String> = []
    for page in 0..<18 {
      coordinateOutsidePanel(surface, frame: surface.frame).hover()
      let snapshot = try surface.snapshot()
      // The panel follows its status item when the menu bar lays out again, as when the clock's hour gains or loses a
      // digit; the viewport keeps its size, so pages captured before and after the move still tile.
      let moved = statusItem.frame
      if snapshot.frame.size == frame.size && snapshot.frame.minY == frame.minY
        && snapshot.frame.minX - frame.minX == moved.minX - anchor.minX
      {
        frame = snapshot.frame
        anchor = moved
      }
      XCTAssertEqual(snapshot.frame, frame, "The viewport changed during paged text capture")
      try application.screenshot().pngRepresentation.write(
        to: output.appendingPathComponent("\(prefix)-\(tab.lowercased())-page-\(page).png"))
      checkedBanners += assertVisibleStringsAndControls(
        in: surface, application: application, snapshot: snapshot, auditedControls: &auditedControls)
      let visible = visibleSnapshots(in: snapshot, within: snapshot.frame)
      for stepper in visible where stepper.elementType == .stepper && stepper.isEnabled {
        XCTAssertFalse(stepper.identifier.isEmpty, "A stepper needs a stable identifier for the interaction audit")
        guard adjustedSteppers.insert(stepper.identifier).inserted else { continue }
        adjustStepperAndRestore(
          surface.steppers[stepper.identifier], in: surface, decrementFirst: stepper.identifier == "pace-workdays")
      }
      let text = visible.map(\.label).filter { !$0.isEmpty }
      let values = visible.compactMap { $0.value as? String }
      exposedText += text + values
      let fingerprint = Set(text + values)
      print("LONG_TEXT_PAGE tab=\(tab) page=\(page) text=\(text.count) values=\(values.count)")
      if !hasScrollView || fingerprint == previousPage
        || tab == "Settings" && visible.contains(where: { $0.elementType == .button && $0.label == "Show Full Log" })
      {
        break
      }
      previousPage = fingerprint
      scroll(scrollView, by: -viewport.height * 0.75)
    }
    if tab == "Usage" {
      if checkedBanners == 0 {
        let attachment = XCTAttachment(string: application.debugDescription)
        attachment.name = "Missing warning banner accessibility tree"
        attachment.lifetime = .keepAlways
        add(attachment)
      }
      XCTAssertGreaterThan(checkedBanners, 0, "No complete warning banner reached the OCR audit")
    }
    if tab == "Settings" {
      let refreshIntervals = Set(ProviderID.allCases.map { "provider-\($0.rawValue)-refresh-interval" })
      XCTAssertTrue(
        refreshIntervals.isSubset(of: adjustedSteppers),
        "The stepper pass skipped \(refreshIntervals.subtracting(adjustedSteppers).sorted())")
    }
    return exposedText
  }

  @MainActor
  func snapshots(in root: any XCUIElementSnapshot) -> [any XCUIElementSnapshot] {
    root.children.flatMap { [$0] + snapshots(in: $0) }
  }

  @MainActor
  func visibleSnapshots(in root: any XCUIElementSnapshot, within clip: CGRect) -> [any XCUIElementSnapshot] {
    let clip = root.elementType == .scrollView ? clip.intersection(root.frame) : clip
    return root.children.flatMap {
      (!$0.frame.isEmpty && clip.contains($0.frame) ? [$0] : []) + visibleSnapshots(in: $0, within: clip)
    }
  }

  @MainActor
  func element(
    for snapshot: any XCUIElementSnapshot, in surface: XCUIElement, tree: any XCUIElementSnapshot
  ) -> XCUIElement {
    let peers = snapshots(in: tree).filter {
      $0.elementType == snapshot.elementType && $0.identifier == snapshot.identifier
    }
    let elements = surface.descendants(matching: snapshot.elementType)
    if !snapshot.identifier.isEmpty && peers.count == 1 {
      return elements.matching(identifier: snapshot.identifier).firstMatch
    }
    let matches = elements.matching(
      NSPredicate(
        format: "identifier == %@ AND label == %@ AND title == %@",
        snapshot.identifier, snapshot.label, snapshot.title))
    return matches.element(
      boundBy: peers.filter { $0.label == snapshot.label && $0.title == snapshot.title }
        .firstIndex { $0.frame == snapshot.frame }!)
  }

  @MainActor
  func tooltipCandidates(in surface: XCUIElement) throws -> [(String, XCUIElement)] {
    let snapshot = try surface.snapshot()
    var controls = visibleSnapshots(in: snapshot, within: snapshot.frame).filter {
      Self.audits($0.elementType, identifier: $0.identifier) && $0.isEnabled
        && !["Usage", "History", "Settings"].contains($0.label)
    }
    guard controls.count >= 5 else { return [] }
    let center = CGPoint(x: snapshot.frame.midX, y: snapshot.frame.midY)
    func take(_ best: ([any XCUIElementSnapshot]) -> (any XCUIElementSnapshot)?) -> XCUIElement {
      let selected = best(controls)!
      let frame = selected.frame
      let type = selected.elementType
      controls.removeAll { $0.frame == frame && $0.elementType == type }
      let control = element(for: selected, in: surface, tree: snapshot)
      return type == .stepper ? control.descendants(matching: .incrementArrow).firstMatch : control
    }
    return [
      ("top", take { $0.min { $0.frame.midY < $1.frame.midY } }),
      ("bottom", take { $0.max { $0.frame.midY < $1.frame.midY } }),
      ("left", take { $0.min { $0.frame.midX < $1.frame.midX } }),
      ("right", take { $0.max { $0.frame.midX < $1.frame.midX } }),
      ("center", take { $0.min { distance($0.frame.center, center) < distance($1.frame.center, center) } }),
    ]
  }

  func waitForTooltip(
    processIdentifier: pid_t, excluding baseline: [CGWindowID: WindowRecord], timeout: TimeInterval
  ) throws -> WindowRecord {
    let deadline = ProcessInfo.processInfo.systemUptime + timeout
    while ProcessInfo.processInfo.systemUptime < deadline {
      if let tooltip = applicationWindows(processIdentifier: processIdentifier).first(where: {
        baseline[$0.key] == nil && $0.value.frame.width <= 420 && $0.value.frame.height <= 240
      })?.value {
        return tooltip
      }
      Thread.sleep(forTimeInterval: 0.016)
    }
    throw ControlAuditError.tooltipMissing
  }

  func applicationWindows(processIdentifier: pid_t) -> [CGWindowID: WindowRecord] {
    guard
      let values = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
        as? [[CFString: Any]]
    else { return [:] }
    return Dictionary(
      uniqueKeysWithValues: values.compactMap { value in
        guard
          (value[kCGWindowOwnerPID] as? NSNumber)?.int32Value == processIdentifier,
          let identifier = (value[kCGWindowNumber] as? NSNumber)?.uint32Value,
          let bounds = value[kCGWindowBounds] as? NSDictionary,
          let frame = CGRect(dictionaryRepresentation: bounds as CFDictionary)
        else { return nil }
        return (identifier, WindowRecord(identifier: identifier, frame: frame))
      })
  }

  @MainActor
  func movePointerOffPanel(in application: XCUIApplication) {
    let surface = application.descendants(matching: .any)["popover-surface"]
    coordinateOutsidePanel(surface, frame: surface.frame).hover()
  }

  @MainActor
  func coordinateOutsidePanel(_ surface: XCUIElement, frame: CGRect) -> XCUICoordinate {
    let screen = CGDisplayBounds(CGMainDisplayID())
    let x = frame.minX > screen.midX ? screen.minX + 5 : screen.maxX - 5
    return surface.coordinate(withNormalizedOffset: .zero)
      .withOffset(CGVector(dx: x - frame.minX, dy: screen.midY - frame.minY))
  }

  func distance(between control: CGRect, and tooltip: CGRect) -> CGFloat {
    let horizontal = max(max(control.minX - tooltip.maxX, tooltip.minX - control.maxX), 0)
    let vertical = max(max(control.minY - tooltip.maxY, tooltip.minY - control.maxY), 0)
    return hypot(horizontal, vertical)
  }

  func distance(_ lhs: CGPoint, _ rhs: CGPoint) -> CGFloat {
    hypot(lhs.x - rhs.x, lhs.y - rhs.y)
  }

  func waitUntil(timeout: TimeInterval, condition: () throws -> Bool) rethrows -> Bool {
    let deadline = ProcessInfo.processInfo.systemUptime + timeout
    while ProcessInfo.processInfo.systemUptime < deadline {
      if try condition() { return true }
      Thread.sleep(forTimeInterval: 0.016)
    }
    return try condition()
  }

  func outputDirectory() throws -> URL {
    let root =
      ProcessInfo.processInfo.environment["TMB_BENCHMARK_OUTPUT_DIR"].map {
        URL(fileURLWithPath: $0, isDirectory: true)
      } ?? FileManager.default.temporaryDirectory.appendingPathComponent("quotaline-live-audit", isDirectory: true)
    let directory = root.appendingPathComponent(name)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
  }

  func write<T: Encodable>(_ value: T, to url: URL) throws {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try encoder.encode(value).write(to: url)
  }

  private static let auditedTypes: Set<XCUIElement.ElementType> = [
    .button, .checkBox, .comboBox, .datePicker, .link, .menuButton, .popUpButton, .radioButton,
    .searchField, .segmentedControl, .slider, .stepper, .switch, .textField,
  ]

  private static func audits(_ type: XCUIElement.ElementType, identifier: String) -> Bool {
    auditedTypes.contains(type) || type == .textView && identifier == "status-template"
  }

  private enum ControlAuditError: Error {
    case tooltipMissing
  }
}

struct ControlAuditRecord: Codable {
  let tab: String
  let type: String
  let identifier: String
  let label: String
  let value: String
  let enabled: Bool
  let interacted: Bool
  let action: String
  let result: String
  let frame: FrameRecord
}

struct FrameRecord: Codable {
  let minX: CGFloat
  let minY: CGFloat
  let width: CGFloat
  let height: CGFloat

  init(_ frame: CGRect) {
    minX = frame.minX
    minY = frame.minY
    width = frame.width
    height = frame.height
  }
}

struct WindowRecord {
  let identifier: CGWindowID
  let frame: CGRect
}

private extension CGRect {
  var center: CGPoint { CGPoint(x: midX, y: midY) }
}
