import AppKit
import CoreGraphics
import Foundation
import QuotalineCore
import XCTest

extension LiveControlAuditUITests {
  @MainActor
  func adjustDate(_ picker: XCUIElement, increasing: Bool) {
    XCTAssertTrue(
      picker.isHittable,
      "Date picker is unreachable: \(picker.debugDescription)")
    picker.click()
    let before = String(describing: picker.value)
    let arrow = picker.descendants(matching: increasing ? .incrementArrow : .decrementArrow).firstMatch
    XCTAssertTrue(
      arrow.exists && arrow.isHittable,
      "Date arrow is unreachable: \(picker.debugDescription)")
    arrow.click()
    XCTAssertTrue(
      waitUntil(timeout: controlTimeout) { String(describing: picker.value) != before },
      "Date value stayed at \(before) after clicking \(increasing ? "increment" : "decrement"): \(picker.debugDescription)"
    )
  }

  @MainActor
  func segmentedControl(
    named label: String, application: XCUIApplication
  ) -> XCUIElement {
    let control = application.descendants(matching: .any).matching(
      NSPredicate(
        format: "label == %@ AND (elementType == %d OR elementType == %d)", label,
        XCUIElement.ElementType.segmentedControl.rawValue, XCUIElement.ElementType.radioGroup.rawValue)
    ).firstMatch
    XCTAssertTrue(control.exists, "Missing segmented control \(label)")
    return control
  }

  @MainActor
  func segment(_ label: String, in control: XCUIElement) -> XCUIElement {
    control.elementType == .radioGroup ? control.radioButtons[label] : control.buttons[label]
  }

  @MainActor
  func segments(in control: XCUIElement) -> [XCUIElement] {
    (control.elementType == .radioGroup ? control.radioButtons : control.buttons).allElementsBoundByIndex
  }

  @MainActor
  func popoverTabs(in application: XCUIApplication) -> XCUIElement {
    application.descendants(matching: .radioGroup)
      .matching(NSPredicate(format: "label == %@", "Popover tabs")).firstMatch
  }

  @MainActor
  func selectMenuItem(
    _ label: String, from picker: XCUIElement, application: XCUIApplication
  ) throws {
    let revealed = reveal(picker, in: application.descendants(matching: .any)["popover-surface"])
    if !revealed {
      let attachment = XCTAttachment(string: application.debugDescription)
      attachment.name = "Unreachable picker \(label), frame \(picker.frame)"
      attachment.lifetime = .keepAlways
      add(attachment)
      let surface = application.descendants(matching: .any)["popover-surface"]
      let viewport = surface.scrollViews.firstMatch.frame.intersection(surface.frame)
      if viewport.contains(picker.frame) {
        picker.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).click()
        let item = application.menuItems[label]
        let opened = item.waitUntilExists(timeout: 2)
        print("PICKER_HIT_DIAGNOSTIC label=\(label) frame=\(picker.frame) coordinateOpenedMenu=\(opened)")
        if opened { item.click() }
      }
    }
    XCTAssertTrue(revealed, "Cannot reach \(label) picker at \(picker.frame)")
    picker.click()
    let item = application.menuItems[label]
    XCTAssertTrue(item.waitUntilExists(timeout: 2), "The picker did not expose \(label)")
    item.click()
    XCTAssertTrue(waitUntil(timeout: controlTimeout) { String(describing: picker.value).contains(label) })
  }

  @MainActor
  func set(_ toggle: XCUIElement, enabled: Bool) {
    if isSelected(toggle) != enabled { toggle.click() }
    XCTAssertTrue(waitUntil(timeout: controlTimeout) { self.isSelected(toggle) == enabled })
  }

  @MainActor
  func toggleAndRestore(_ toggle: XCUIElement, in surface: XCUIElement) {
    let before = isSelected(toggle)
    toggle.click()
    XCTAssertTrue(waitUntil(timeout: controlTimeout) { self.isSelected(toggle) != before })
    // Toggling can remove rows and move the checkbox within its scroll view.
    XCTAssertTrue(reveal(toggle, in: surface), "Cannot reach the checkbox after toggling")
    var frame = toggle.frame
    XCTAssertTrue(
      waitUntil(timeout: controlTimeout) {
        let current = toggle.frame
        defer { frame = current }
        return current == frame && toggle.isHittable
      }, "The checkbox did not settle after toggling")
    // Element clicks can auto-scroll a remounted checkbox out of the viewport even after hit testing succeeds.
    toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).click()
    XCTAssertTrue(waitUntil(timeout: controlTimeout) { self.isSelected(toggle) == before })
  }

  /// The model list is lazy, so rows scrolled out of view unmount and a surface snapshot can hold stale values for
  /// them. Filtering by provider name keeps the provider's header and all of its rows mounted together, so the record
  /// is taken before the filter clears.
  @MainActor
  func toggleModelGroupAndRestore(
    _ provider: ProviderID, surface: XCUIElement, application: XCUIApplication
  ) throws -> ControlAuditRecord {
    let toggle = application.checkBoxes["Show all \(provider.displayName) models"]
    let identifiers = DemoData.snapshot(provider, now: Date(), fixture: .controlAudit).windows.map {
      "model-selection-\(provider.rawValue):\($0.id)"
    }
    let filter = application.textFields["model-filter"]
    XCTAssertTrue(reveal(filter, in: surface))
    replaceText(in: filter, with: provider.displayName, application: application)
    defer {
      XCTAssertTrue(reveal(filter, in: surface))
      replaceText(in: filter, with: "", application: application)
    }
    XCTAssertTrue(reveal(application.checkBoxes[try XCTUnwrap(identifiers.last)], in: surface))
    XCTAssertTrue(reveal(toggle, in: surface), "Missing \(provider.displayName) model select-all")
    let before = try XCTUnwrap(
      try mountedModelStates(provider, in: surface),
      "\(provider.displayName) model rows do not fit the viewport together")
    XCTAssertEqual(Set(before.keys), Set(identifiers), "Missing model checkboxes for \(provider.displayName)")
    let selected = !before.values.allSatisfy { $0 }
    for (expected, action) in [(selected, "Selecting"), (!selected, "Clearing")] {
      toggle.click()
      XCTAssertTrue(
        waitUntil(timeout: groupChangeTimeout) {
          (try? self.mountedModelStates(provider, in: surface)) == before.mapValues { _ in expected }
        },
        "\(action) \(provider.displayName) left \(String(describing: try? mountedModelStates(provider, in: surface)))")
    }
    for identifier in before.keys.sorted() where before[identifier] != !selected {
      application.checkBoxes[identifier].click()
    }
    XCTAssertTrue(
      waitUntil(timeout: controlTimeout) { (try? self.mountedModelStates(provider, in: surface)) == before },
      "Restoring \(provider.displayName) left \(String(describing: try? mountedModelStates(provider, in: surface)))")
    return scenarioRecord(
      tab: "Settings", label: "\(provider.displayName) model select-all", element: toggle,
      action: "select all, clear, and restore models")
  }

  /// Reads one provider's rows from a single snapshot, or nil while any of them lies outside the viewport.
  @MainActor
  func mountedModelStates(_ provider: ProviderID, in surface: XCUIElement) throws -> [String: Bool]? {
    let root = try surface.snapshot()
    let elements = snapshots(in: root)
    let viewport = (elements.first { $0.elementType == .scrollView }?.frame ?? root.frame).intersection(root.frame)
    let rows = elements.filter {
      $0.elementType == .checkBox && $0.identifier.hasPrefix("model-selection-\(provider.rawValue):")
    }
    guard rows.allSatisfy({ viewport.contains($0.frame) }) else { return nil }
    return Dictionary(uniqueKeysWithValues: rows.map { ($0.identifier, checked($0.value)) })
  }

  @MainActor
  func incrementStepper(_ stepper: XCUIElement) {
    let before = String(describing: stepper.value)
    let increment = stepper.descendants(matching: .incrementArrow).firstMatch
    XCTAssertTrue(increment.exists, "Stepper did not expose an increment arrow")
    XCTAssertTrue(increment.isHittable)
    increment.click()
    XCTAssertTrue(waitUntil(timeout: controlTimeout) { String(describing: stepper.value) != before })
  }

  @MainActor
  func adjustStepperAndRestore(
    _ stepper: XCUIElement, in surface: XCUIElement, decrementFirst: Bool = false
  ) {
    let increment = stepper.descendants(matching: .incrementArrow).firstMatch
    let decrement = stepper.descendants(matching: .decrementArrow).firstMatch
    XCTAssertTrue(reveal(decrementFirst ? decrement : increment, in: surface))
    XCTAssertTrue(increment.exists && decrement.exists, "Stepper did not expose increment and decrement arrows")
    let before = String(describing: stepper.value)
    (decrementFirst ? decrement : increment).click()
    XCTAssertTrue(
      waitUntil(timeout: controlTimeout) { String(describing: stepper.value) != before },
      "Stepper \(stepper.identifier) did not change from \(before)")
    // A pointer left on the first arrow raises this control's rich tooltip over the other one.
    coordinateOutsidePanel(surface, frame: surface.frame).hover()
    XCTAssertTrue(reveal(decrementFirst ? increment : decrement, in: surface))
    (decrementFirst ? increment : decrement).click()
    XCTAssertTrue(
      waitUntil(timeout: controlTimeout) { String(describing: stepper.value) == before },
      "Stepper \(stepper.identifier) did not restore \(before); received \(String(describing: stepper.value))")
  }

  @MainActor
  func isSelected(_ element: XCUIElement) -> Bool {
    checked(element.value)
  }

  func checked(_ value: Any?) -> Bool {
    let text = (value as? NSNumber)?.stringValue ?? (value as? String)?.lowercased()
    switch text {
    case "1", "on", "selected", "true": return true
    case "0", "off", "unselected", "false": return false
    default:
      XCTFail("Expected a binary control value, received \(String(describing: value))")
      return false
    }
  }

  @MainActor
  func replaceText(in field: XCUIElement, with value: String, application: XCUIApplication) {
    field.click()
    application.typeKey("a", modifierFlags: .command)
    if value.isEmpty {
      application.typeKey(.delete, modifierFlags: [])
    } else {
      field.typeText(value)
    }
    XCTAssertTrue(
      waitUntil(timeout: controlTimeout) { (field.value as? String) == value },
      "Expected editable text \(String(reflecting: value)); received \(String(describing: field.value))")
  }

  /// Scrolls the whole distance to the element at once and repeats only to correct for wheel acceleration. Each pass
  /// reads one snapshot, since every separate attribute read is a round trip through the accessibility tree.
  @MainActor
  func reveal(_ element: XCUIElement, in surface: XCUIElement) -> Bool {
    let scrollView = surface.scrollViews.firstMatch
    guard scrollView.exists else { return element.exists && element.isHittable }
    let surfaceFrame = surface.frame
    let viewport = scrollView.frame.intersection(surfaceFrame).insetBy(dx: 2, dy: 2)
    if !element.exists { scrollToTop(surface) }
    for _ in 0..<20 {
      if element.exists { break }
      let first = scrollView.staticTexts.firstMatch
      guard let anchor = try? first.snapshot() else { return false }
      guard
        scroll(
          scrollView, by: -viewport.height * 0.75,
          until: {
            element.exists
              || (try? first.snapshot()).map { $0.label != anchor.label || $0.frame != anchor.frame } == true
          })
      else { return false }
    }
    guard var current = try? element.snapshot() else { return false }
    let type = current.elementType
    if type == .stepper {
      return reveal(element.descendants(matching: .incrementArrow).firstMatch, in: surface)
    }
    if type == .radioGroup || type == .segmentedControl {
      guard let segment = segments(in: element).first else { return false }
      return reveal(segment, in: surface)
    }
    if current.identifier.hasPrefix("footer-") {
      return surfaceFrame.contains(current.frame) && element.isHittable
    }
    for _ in 0..<20 {
      let before = current.frame
      if viewport.contains(before) {
        if type == .staticText || waitUntil(timeout: controlTimeout, condition: { element.isHittable }) {
          return true
        }
        // Wheel acceleration can carry a briefly visible control past the viewport while hit testing waits.
        guard let after = try? element.snapshot(), !viewport.contains(after.frame) else { return false }
        current = after
        continue
      }
      guard before.midX >= viewport.minX && before.midX <= viewport.maxX else { return false }
      guard abs(before.midY - viewport.midY) > 20 else { return false }
      let anchor = scrollView.staticTexts.firstMatch
      let anchorBefore = anchor.frame
      var after: (any XCUIElementSnapshot)?
      guard
        scroll(
          scrollView, by: viewport.midY - before.midY,
          until: {
            after = try? element.snapshot()
            return (after.map { $0.frame != before } ?? true) || anchor.frame != anchorBefore
          })
      else {
        let attachment = XCTAttachment(
          string:
            "target=\(current.identifier) before=\(before) after=\(element.frame) "
            + "anchorBefore=\(anchorBefore) anchorAfter=\(anchor.frame) viewport=\(viewport)")
        attachment.name = "Scroll made no progress"
        attachment.lifetime = .keepAlways
        add(attachment)
        let screenshot = XCTAttachment(screenshot: surface.screenshot())
        screenshot.name = "Unmoved scroll viewport"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        return false
      }
      guard let after else { return false }
      current = after
    }
    return false
  }

  @MainActor
  func scrollToTop(_ surface: XCUIElement) {
    let scrollView = surface.scrollViews.firstMatch
    guard scrollView.exists else { return }
    let firstText = scrollView.staticTexts.firstMatch
    XCTAssertTrue(firstText.exists, "The tab has no accessible first content row")
    guard firstText.exists else { return }
    let viewport = scrollView.frame.intersection(surface.frame)
    scroll(scrollView, by: max(viewport.height, viewport.minY - firstText.frame.minY + viewport.height))
    XCTAssertTrue(
      reveal(firstText, in: surface),
      "Scrolling did not expose the first content row")
  }

  /// XCTest never confirms that a synthesized wheel gesture reached the scroll view, so a viewport that did not move
  /// gets the gesture once more; a late first gesture only overshoots, which the caller's next pass measures and undoes.
  @MainActor
  func scroll(_ scrollView: XCUIElement, by delta: CGFloat, until moved: () -> Bool) -> Bool {
    for _ in 0..<2 {
      scroll(scrollView, by: delta)
      if waitUntil(timeout: controlTimeout, condition: moved) { return true }
    }
    return false
  }

  @MainActor
  func scroll(_ scrollView: XCUIElement, by delta: CGFloat) {
    // The center can sit over a nested scroller or control that consumes the wheel; the leading padding holds neither.
    let pointer = scrollView.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0.5))
      .withOffset(CGVector(dx: PopoverGeometry.contentPadding / 2, dy: 0))
    pointer.hover()
    pointer.scroll(byDeltaX: 0, deltaY: delta)
  }

  @MainActor
  func collectButtonLabels(prefix: String, in surface: XCUIElement) -> Set<String> {
    Set(
      surface.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", prefix)).allElementsBoundByIndex.map(\.label))
  }

  @MainActor
  func contentSignature(_ element: XCUIElement) throws -> String {
    let snapshot = try element.snapshot()
    let frame = snapshot.frame
    var display: CGDirectDisplayID = 0
    var count: UInt32 = 0
    let onScreen =
      frame.origin.x.isFinite && frame.origin.y.isFinite && frame.width.isFinite && frame.height.isFinite
      && !frame.isEmpty && CGGetDisplaysWithRect(frame, 1, &display, &count) == .success
      && count == 1 && CGDisplayBounds(display).contains(frame)
    _ = try XCTUnwrap(onScreen ? true : nil, "Status-item capture requires a visible item; reported frame: \(frame)")
    let value = String(describing: snapshot.value)
    let pixels = element.screenshot().pngRepresentation
    return "\(snapshot.label)|\(value)|\(pixels.count)|\(pixels.hashValue)"
  }

  @MainActor
  func assertAnchorsHeld(
    statusItem: XCUIElement, statusFrame: CGRect, surface: XCUIElement, panelFrame: CGRect, action: String
  ) throws {
    let status = try statusItem.snapshot()
    let panel = try surface.snapshot()
    XCTAssertFalse(status.label.isEmpty, "\(action) removed the status-item accessibility label")
    XCTAssertLessThan(abs(status.frame.minX - statusFrame.minX), 2, "\(action) moved the status item")
    XCTAssertLessThan(abs(status.frame.width - statusFrame.width), 2, "\(action) resized the open status item")
    XCTAssertLessThan(abs(panel.frame.minX - panelFrame.minX), 2, "\(action) moved the panel horizontally")
    XCTAssertLessThan(abs(panel.frame.minY - panelFrame.minY), 2, "\(action) moved the panel top edge")
  }

  @MainActor
  func retainChooserScreenshot(_ application: XCUIApplication, named name: String) {
    let cancel = application.buttons["CancelButton"]
    XCTAssertTrue(cancel.waitUntilExists(timeout: 2))
    XCTAssertTrue(cancel.isHittable)
    let attachment = XCTAttachment(screenshot: application.screenshot())
    attachment.name = name
    attachment.lifetime = .keepAlways
    add(attachment)
  }

  @MainActor
  func assertAndCancelNativePanel(_ application: XCUIApplication, rootedAt directory: URL) {
    let panel = application.descendants(matching: .any).matching(
      NSPredicate(format: "identifier IN %@", ["save-panel", "open-panel"])
    ).firstMatch
    let presented = panel.waitUntilExists(timeout: 2)
    XCTAssertTrue(presented, "The action did not present an on-screen native panel")
    guard presented else { return }
    let location = panel.popUpButtons["where popup"]
    let locationValue = location.value as? String ?? ""
    XCTAssertTrue(
      location.exists && directory.lastPathComponent.hasPrefix(locationValue.replacingOccurrences(of: "...", with: "")),
      "The native panel is not rooted in \(directory.path): \(locationValue)")
    let cancel = panel.buttons["CancelButton"]
    let canCancel = cancel.isHittable
    XCTAssertTrue(canCancel, "The native panel did not expose an on-screen Cancel button")
    guard canCancel else { return }
    cancel.click()
    XCTAssertTrue(cancel.waitForNonExistence(timeout: 2))
    activate(application)
    XCTAssertTrue(application.descendants(matching: .any)["popover-surface"].waitUntilExists(timeout: 2))
  }

  /// Hit testing spans every application on screen, so a window of the runner or of whichever application a native
  /// panel handed activation back to can cover a control this one still reports as visible.
  ///
  @MainActor
  func activate(_ application: XCUIApplication) {
    if application.state != .runningForeground { application.activate() }
    XCTAssertTrue(
      waitUntil(timeout: controlTimeout) { application.state == .runningForeground },
      "The application left the foreground, so its controls cannot be reached")
  }

  @MainActor
  func scenarioRecord(
    tab: String, label: String, element: XCUIElement, action: String = "interact"
  ) -> ControlAuditRecord {
    do {
      let snapshot = try element.snapshot()
      return ControlAuditRecord(
        tab: tab, type: "scenario", identifier: snapshot.identifier, label: label,
        value: String(describing: snapshot.value), enabled: snapshot.isEnabled,
        interacted: true, action: action, result: "passed", frame: FrameRecord(snapshot.frame))
    } catch {
      XCTFail("Could not record \(label): \(error)")
      return ControlAuditRecord(
        tab: tab, type: "scenario", identifier: "", label: label, value: "", enabled: false,
        interacted: true, action: action, result: "failed: \(error)", frame: FrameRecord(.zero))
    }
  }

  func absenceRecord(tab: String, label: String, action: String) -> ControlAuditRecord {
    ControlAuditRecord(
      tab: tab, type: "absence", identifier: label, label: label, value: "absent", enabled: false,
      interacted: false, action: action, result: "passed", frame: FrameRecord(.zero))
  }

  func assertRequiredInventory(
    _ records: [ControlAuditRecord], tab: String, section: SettingsSection?, modelSelectionOnly: Bool
  ) {
    let recorded = Set(records.map { "\($0.tab)|\($0.label)" })
    var required: Set<String> = [
      "Usage|Refresh all providers", "Usage|Usage-site links", "Usage|Sign-in prompts",
      "History|UTC boundaries", "History|Window Usage rollups", "History|Additive metric stacking",
      "History|Previous, next, and Now", "History|Custom From and To",
      "History|Export CSV save panel Cancel", "Settings|Version and build", "Settings|Distribution channel",
      "Settings|Reset All Settings Cancel", "Settings|Launch at login", "Settings|Open Login Items",
      "Settings|Copy Diagnostics", "Settings|Report Issue", "Settings|Source", "Settings|Privacy Policy",
      "Settings|Acknowledgements",
      "Settings|Direct update controls", "Settings|Model order", "Settings|Status format", "Settings|Decimals",
      "Settings|Template and tokens", "Settings|Live menu bar preview", "Settings|Model filter",
      "Settings|Command-F model filter",
      "Settings|Hide unused models", "Settings|Model selection", "Settings|Revert short label",
      "Settings|Stable order move later and earlier", "Settings|Hide 0%", "Settings|Fit to space",
      "Settings|Show all providers", "Settings|Provider token refresh", "Settings|History retention",
      "Settings|Analytics refresh", "Settings|Full history path", "Settings|History Open",
      "Settings|History Export", "Settings|Clear History confirmation", "Settings|Notifications enabled",
      "Settings|Window resets", "Settings|Sign-in needed", "Settings|Detailed logging", "Settings|Log level",
      "Settings|Log search", "Settings|Log Copy", "Settings|Log Clear", "Settings|Show Full Log",
      "Settings|Log view", "Settings|Command-R refresh", "Settings|Full log Command-F search",
    ]
    for threshold in ["50%", "75%", "90%", "100%"] {
      required.insert("Settings|Notification threshold \(threshold)")
    }
    for provider in ProviderID.allCases {
      required.formUnion([
        "Usage|Refresh \(provider.displayName)", "Settings|\(provider.displayName) model select-all",
        "Settings|\(provider.displayName) enabled", "Settings|\(provider.displayName) refresh interval",
        "Settings|\(provider.displayName) authentication source", "Settings|\(provider.displayName) setup guide",
      ])
    }
    required = required.filter {
      let label = String($0.dropFirst(9))
      return $0.hasPrefix("\(tab)|") && (section == nil || settingsSection(for: label) == section)
        && (section != .menuBar || isModelSelectionControl(label) == modelSelectionOnly)
    }
    XCTAssertTrue(
      required.isSubset(of: recorded),
      "Control matrix omitted: \(required.subtracting(recorded).sorted().joined(separator: ", "))")
  }

  func settingsSection(for label: String) -> SettingsSection {
    switch label {
    case "Version and build", "Distribution channel", "Reset All Settings Cancel", "Launch at login",
      "Open Login Items", "Copy Diagnostics", "Report Issue", "Source", "Privacy Policy", "Acknowledgements",
      "Direct update controls":
      return .about
    case "Show all providers", "Provider token refresh":
      return .providers
    case "History retention", "Analytics refresh", "Full history path", "History Open", "History Export",
      "Clear History confirmation":
      return .data
    case "Notifications enabled", "Window resets", "Sign-in needed":
      return .notifications
    case "Detailed logging", "Log level", "Log search", "Log Copy", "Log Clear", "Show Full Log", "Log view",
      "Command-R refresh", "Full log Command-F search":
      return .log
    default:
      if label.hasPrefix("Notification threshold ") { return .notifications }
      if [" enabled", " refresh interval", " authentication source", " setup guide"].contains(where: label.hasSuffix) {
        return .providers
      }
      return .menuBar
    }
  }

  func isModelSelectionControl(_ label: String) -> Bool {
    ["Model filter", "Command-F model filter", "Hide unused models", "Model selection"].contains(label)
      || label.hasSuffix(" model select-all")
  }
}
