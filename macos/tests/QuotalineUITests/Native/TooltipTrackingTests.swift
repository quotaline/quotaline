import AppKit
import QuotalineTestSupport
import SwiftUI
import Testing

@testable import QuotalineCore
@testable import QuotalineUI

@Suite(.serialized)
struct TooltipTrackingTests {
  @Test @MainActor func tooltipPresenterRejectsAWindowChangedDuringTheDelay() async {
    let gate = TooltipSleepGate()
    let panel = TooltipPanelSpy()
    let presenter = TooltipPresenter(sleep: { await gate.sleep($0) }, panelFactory: { panel })
    let firstWindow = NSWindow()
    let secondWindow = NSWindow()
    let source = TooltipSourceStub(presenter: presenter, title: "Moved", window: firstWindow)

    presenter.arm(source: source)
    while await gate.durations.isEmpty { await Task.yield() }
    source.tooltipPresentationContext = TooltipPresentationContext(
      anchorRect: CGRect(x: 120, y: 220, width: 40, height: 20),
      visibleFrame: CGRect(x: 0, y: 0, width: 800, height: 600),
      parentWindow: secondWindow
    )
    await gate.releaseAll()
    await presenter.settle()

    #expect(panel.contents.isEmpty)
    #expect(presenter.visibleOwner == nil)
    presenter.tearDown()
  }

  @Test @MainActor func tooltipPresenterDismissesOnOwnerWindowGeometryChanges() async {
    let panel = TooltipPanelSpy()
    let presenter = TooltipPresenter(sleep: { _ in }, panelFactory: { panel })
    let window = NSWindow()
    let source = TooltipSourceStub(presenter: presenter, title: "Moved", window: window)

    presenter.arm(source: source)
    await presenter.settle()
    NotificationCenter.default.post(name: NSWindow.didMoveNotification, object: window)
    await Task.yield()
    await Task.yield()

    #expect(panel.anchorRects == [CGRect(x: 100, y: 200, width: 40, height: 20)])
    #expect(!panel.isVisible)
    #expect(presenter.visibleOwner == nil)
    presenter.tearDown()
  }

  @Test @MainActor func tooltipPresenterDismissesWhenTheAnchorBecomesInvalidDuringResize() async {
    let panel = TooltipPanelSpy()
    let presenter = TooltipPresenter(sleep: { _ in }, panelFactory: { panel })
    let window = NSWindow()
    let source = TooltipSourceStub(presenter: presenter, title: "Removed", window: window)

    presenter.arm(source: source)
    await presenter.settle()
    source.tooltipPresentationContext = nil
    NotificationCenter.default.post(name: NSWindow.didResizeNotification, object: window)
    await Task.yield()
    await Task.yield()

    #expect(presenter.visibleOwner == nil)
    #expect(!panel.isVisible)
    presenter.tearDown()
  }

  @Test @MainActor func tooltipTrackingViewRefreshesVisibleContentInItsWindow() async throws {
    let screen = try #require(NSScreen.screens.first)
    let panel = TooltipPanelSpy()
    let presenter = TooltipPresenter(sleep: { _ in }, panelFactory: { panel })
    let initial = TooltipContent(title: "Initial", body: "First explanation")
    let view = TooltipTrackingView(content: initial, presenter: presenter)
    let frame = CGRect(x: screen.visibleFrame.midX, y: screen.visibleFrame.midY, width: 160, height: 40)
    let window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: false)
    window.alphaValue = 0
    window.contentView = view
    view.frame = CGRect(origin: .zero, size: frame.size)
    window.orderFrontRegardless()

    view.update(content: initial, focused: true)
    let context = try #require(view.tooltipPresentationContext)
    #expect(context.parentWindow === window)
    #expect(!context.anchorRect.isEmpty)

    await presenter.settle()
    let updated = TooltipContent(title: "Updated", body: "Second explanation")
    view.update(content: updated, focused: true)

    #expect(panel.contents == [initial, updated])
    #expect(panel.reduceMotionValues.last == true)
    window.contentView = nil
    #expect(presenter.visibleOwner == nil)
    view.dismantle()
    presenter.tearDown()
  }

  @Test @MainActor func tooltipTrackingViewKeepsFocusedHelpAfterPointerExit() async throws {
    let gate = TooltipSleepGate()
    let presenter = TooltipPresenter(
      sleep: { await gate.sleep($0) },
      panelFactory: { TooltipPanelSpy() }
    )
    let view = TooltipTrackingView(
      content: TooltipContent(title: "Refresh", body: "Fetches current limits"),
      presenter: presenter
    )
    let window = try showInvisibleWindow(containing: view)
    view.update(content: view.tooltipContent, focused: true)
    while await gate.durations.isEmpty { await Task.yield() }

    let exit = try #require(
      NSEvent.mouseEvent(
        with: .mouseMoved,
        location: .zero,
        modifierFlags: [],
        timestamp: 0,
        windowNumber: 0,
        context: nil,
        eventNumber: 0,
        clickCount: 0,
        pressure: 0
      )
    )
    window.mouseLocation = CGPoint(x: 400, y: 30)
    view.mouseExited(with: exit)
    #expect(presenter.hasPendingTask)

    view.update(content: view.tooltipContent, focused: false)
    await gate.releaseAll()
    await presenter.settle()
    #expect(!presenter.hasPendingTask)
    view.dismantle()
    presenter.tearDown()
    window.orderOut(nil)
  }

  @Test @MainActor func tooltipTrackingViewDismissesHelpWhenHidden() async throws {
    let panel = TooltipPanelSpy()
    let presenter = TooltipPresenter(sleep: { _ in }, panelFactory: { panel })
    let view = TooltipTrackingView(
      content: TooltipContent(title: "Refresh", body: "Fetches current limits"),
      presenter: presenter
    )
    let window = try showInvisibleWindow(containing: view)
    view.update(content: view.tooltipContent, focused: true)
    await presenter.settle()
    #expect(panel.isVisible)

    view.viewDidHide()

    #expect(!panel.isVisible)
    #expect(presenter.visibleOwner == nil)
    view.dismantle()
    presenter.tearDown()
    window.orderOut(nil)
  }

  @Test @MainActor func tooltipTrackingViewTransitionsBetweenPointerAndFocusWithoutRearming() async throws {
    let gate = TooltipSleepGate()
    let presenter = TooltipPresenter(
      sleep: { await gate.sleep($0) },
      panelFactory: { TooltipPanelSpy() }
    )
    let view = TooltipTrackingView(
      content: TooltipContent(title: "Refresh", body: "Fetches current limits"),
      presenter: presenter
    )
    let window = try showInvisibleWindow(containing: view)
    view.update(content: view.tooltipContent, focused: true)
    while await gate.durations.isEmpty { await Task.yield() }
    view.mouseEntered(with: try mouseEvent())
    window.mouseLocation = CGPoint(x: 400, y: 30)
    view.mouseExited(with: try mouseEvent())

    #expect(await gate.durations == [TooltipTiming.presentationDelay])
    #expect(presenter.hasPendingTask)
    await gate.releaseAll()
    await presenter.settle()
    #expect(!presenter.hasPendingTask)
    view.dismantle()
    presenter.tearDown()
    window.orderOut(nil)
  }

  @Test @MainActor func tooltipTrackingViewPresentsHelpWhenThePointerIsInsideWithoutAnEnterEvent() async throws {
    let (presenter, view, window) = try hoverFixture(applicationIsActive: true)

    presenter.syncHover(windowNumber: window.windowNumber)
    await presenter.settle()

    #expect(presenter.visibleOwner == view.tooltipOwner)
    tearDownHoverFixture(presenter, view, window)
  }

  @Test @MainActor func tooltipTrackingViewIgnoresAnExitEventWhileThePointerStaysInside() async throws {
    let (presenter, view, window) = try hoverFixture(applicationIsActive: true)
    view.mouseEntered(with: try mouseEvent())
    await presenter.settle()

    view.mouseExited(with: try mouseEvent())
    await presenter.settle()

    #expect(presenter.visibleOwner == view.tooltipOwner)
    tearDownHoverFixture(presenter, view, window)
  }

  @Test @MainActor func tooltipTrackingViewDismissesHelpWhenThePointerLeftWithoutAnExitEvent() async throws {
    let (presenter, view, window) = try hoverFixture(applicationIsActive: true)
    view.mouseEntered(with: try mouseEvent())
    await presenter.settle()

    window.mouseLocation = CGPoint(x: 400, y: 30)
    presenter.syncHover(windowNumber: window.windowNumber)
    await presenter.settle()

    #expect(presenter.visibleOwner == nil)
    tearDownHoverFixture(presenter, view, window)
  }

  @Test @MainActor func tooltipTrackingViewIgnoresThePointerOverAHiddenView() async throws {
    let (presenter, view, window) = try hoverFixture(applicationIsActive: true)
    view.isHidden = true

    presenter.syncHover(windowNumber: window.windowNumber)
    await presenter.settle()

    #expect(presenter.visibleOwner == nil)
    tearDownHoverFixture(presenter, view, window)
  }

  @Test @MainActor func tooltipTrackingViewHonorsAnExitOntoAWindowAboveIt() async throws {
    let (presenter, view, window) = try hoverFixture(applicationIsActive: true, coveredByAnotherWindow: true)
    view.mouseEntered(with: try mouseEvent())
    await presenter.settle()

    view.mouseExited(with: try mouseEvent())
    await presenter.settle()

    #expect(presenter.visibleOwner == nil)
    tearDownHoverFixture(presenter, view, window)
  }

  @Test @MainActor func tooltipPresenterAsksTheWindowServerForTheWindowUnderThePointer() throws {
    let window = try showInvisibleWindow(containing: NSView())
    let point = CGPoint(x: window.frame.midX, y: window.frame.midY)
    let presenter = TooltipPresenter()

    // Another process's window can sit above the test window on a shared desktop, so compare with AppKit's answer.
    #expect(presenter.topWindowNumber(point) == NSWindow.windowNumber(at: point, belowWindowWithWindowNumber: 0))
    presenter.tearDown()
    window.orderOut(nil)
  }

  @Test @MainActor func tooltipPresenterKeepsHoverRecoveryAcrossTeardown() throws {
    let (presenter, view, window) = try hoverFixture(applicationIsActive: true)

    presenter.tearDown()

    #expect(presenter.hasHoverMonitor)
    tearDownHoverFixture(presenter, view, window)
  }

  @Test @MainActor func tooltipTrackingViewIgnoresThePointerWhileTheApplicationIsInactive() async throws {
    let (presenter, view, window) = try hoverFixture(applicationIsActive: false)

    presenter.syncHover(windowNumber: window.windowNumber)
    await presenter.settle()

    #expect(presenter.visibleOwner == nil)
    tearDownHoverFixture(presenter, view, window)
  }

  @MainActor
  private func hoverFixture(
    applicationIsActive: Bool, coveredByAnotherWindow: Bool = false
  ) throws -> (TooltipPresenter, TooltipTrackingView, TooltipMouseWindow) {
    let topWindow = TopWindowStub()
    let presenter = TooltipPresenter(
      sleep: { _ in }, panelFactory: { TooltipPanelSpy() }, applicationIsActive: { applicationIsActive },
      topWindowNumber: { _ in topWindow.number })
    let view = TooltipTrackingView(
      content: TooltipContent(title: "Refresh", body: "Fetches current limits"),
      presenter: presenter
    )
    let window = try showInvisibleWindow(containing: view)
    topWindow.number = coveredByAnotherWindow ? window.windowNumber + 1 : window.windowNumber
    return (presenter, view, window)
  }

  @MainActor
  private func tearDownHoverFixture(_ presenter: TooltipPresenter, _ view: TooltipTrackingView, _ window: NSWindow) {
    view.dismantle()
    presenter.tearDown()
    window.orderOut(nil)
  }
}

@MainActor
private final class TopWindowStub {
  var number = 0
}
