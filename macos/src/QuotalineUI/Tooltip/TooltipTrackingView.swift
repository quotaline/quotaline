import AppKit
import QuotalineCore
import SwiftUI

@MainActor
final class TooltipTrackingView: NSView, TooltipPresentationSource {
  let tooltipOwner: TooltipOwner
  private weak var presenter: TooltipPresenter?
  private var focused = false
  private var hovering = false
  private var tracking: NSTrackingArea?
  var tooltipContent: TooltipContent
  var tooltipPresentationDelay = TooltipTiming.presentationDelay

  init(content: TooltipContent, presenter: TooltipPresenter, tracksHover: Bool = true) {
    tooltipOwner = presenter.makeOwner()
    tooltipContent = content
    self.presenter = presenter
    super.init(frame: .zero)
    if tracksHover {
      let tracking = NSTrackingArea(
        rect: .zero,
        options: [.activeInActiveApp, .inVisibleRect, .mouseEnteredAndExited],
        owner: self,
        userInfo: nil
      )
      addTrackingArea(tracking)
      self.tracking = tracking
      presenter.trackHover(of: self)
    }
    setAccessibilityElement(false)
    setAccessibilityHidden(true)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is unavailable")
  }

  override var acceptsFirstResponder: Bool { false }

  override func hitTest(_ point: NSPoint) -> NSView? { nil }

  var tooltipPresentationContext: TooltipPresentationContext? {
    guard
      let window,
      window.isVisible,
      !isHiddenOrHasHiddenAncestor,
      let screen = window.screen
    else { return nil }
    let visibleBounds = visibleRect.intersection(bounds)
    guard !visibleBounds.isNull, !visibleBounds.isEmpty else { return nil }

    let anchorRect: CGRect
    if hovering {
      let mouseInWindow = window.mouseLocationOutsideOfEventStream
      guard visibleBounds.contains(convert(mouseInWindow, from: nil)) else { return nil }
      let mouseOnScreen = window.convertToScreen(CGRect(origin: mouseInWindow, size: .zero)).origin
      anchorRect = CGRect(origin: mouseOnScreen, size: CGSize(width: 1, height: 1))
    } else if focused {
      anchorRect = window.convertToScreen(convert(visibleBounds, to: nil))
    } else {
      return nil
    }
    let clippedAnchor = anchorRect.intersection(screen.visibleFrame)
    guard !clippedAnchor.isNull, !clippedAnchor.isEmpty else { return nil }
    return TooltipPresentationContext(
      anchorRect: clippedAnchor,
      visibleFrame: screen.visibleFrame,
      parentWindow: window
    )
  }

  var tooltipClipView: NSClipView? {
    enclosingScrollView?.contentView
  }

  override func mouseEntered(with event: NSEvent) {
    hovering = true
    presenter?.update(source: self, hovering: hovering, focused: focused)
  }

  override func mouseExited(with event: NSEvent) {
    guard !pointerIsInside else { return }
    hovering = false
    presenter?.update(source: self, hovering: hovering, focused: focused)
  }

  func syncHoverWithPointer() {
    let inside = pointerIsInside
    guard inside != hovering else { return }
    hovering = inside
    presenter?.update(source: self, hovering: hovering, focused: focused)
  }

  private var pointerIsInside: Bool {
    guard let window, let presenter, window.isVisible, !isHiddenOrHasHiddenAncestor, presenter.applicationIsActive()
    else { return false }
    let pointer = window.mouseLocationOutsideOfEventStream
    let visibleBounds = visibleRect.intersection(bounds)
    return !visibleBounds.isEmpty && visibleBounds.contains(convert(pointer, from: nil))
      && presenter.topWindowNumber(window.convertPoint(toScreen: pointer)) == window.windowNumber
  }

  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    if window == nil {
      hovering = false
      presenter?.dismiss(owner: tooltipOwner)
    } else {
      presenter?.refresh(source: self)
    }
  }

  override func viewDidMoveToSuperview() {
    super.viewDidMoveToSuperview()
    presenter?.refresh(source: self)
  }

  override func viewDidHide() {
    super.viewDidHide()
    hovering = false
    presenter?.dismiss(owner: tooltipOwner)
  }

  override func setFrameOrigin(_ newOrigin: NSPoint) {
    let changed = frame.origin != newOrigin
    super.setFrameOrigin(newOrigin)
    if changed { presenter?.refresh(source: self) }
  }

  override func setFrameSize(_ newSize: NSSize) {
    let changed = frame.size != newSize
    super.setFrameSize(newSize)
    if changed { presenter?.refresh(source: self) }
  }

  func update(content: TooltipContent, focused: Bool) {
    let contentChanged = tooltipContent != content
    let focusChanged = focused != self.focused
    tooltipContent = content
    self.focused = focused
    if focusChanged {
      presenter?.update(source: self, hovering: hovering, focused: focused)
    } else if contentChanged, focused || hovering {
      presenter?.refresh(source: self)
    }
  }

  func dismantle() {
    presenter?.dismiss(owner: tooltipOwner)
    focused = false
    hovering = false
    presenter = nil
    if let tracking { removeTrackingArea(tracking) }
    tracking = nil
  }
}

struct TooltipAnchor: NSViewRepresentable {
  @Environment(\.tooltipPresentationDelay) private var presentationDelay
  let content: TooltipContent
  let focused: Bool
  let presenter: TooltipPresenter
  let tracksHover: Bool

  func makeNSView(context: Context) -> TooltipTrackingView {
    TooltipTrackingView(content: content, presenter: presenter, tracksHover: tracksHover)
  }

  func updateNSView(_ view: TooltipTrackingView, context: Context) {
    view.tooltipPresentationDelay = presentationDelay
    view.update(content: content, focused: focused)
  }

  static func dismantleNSView(_ view: TooltipTrackingView, coordinator: Void) {
    view.dismantle()
  }
}

extension EnvironmentValues {
  var tooltipPresentationDelay: Duration {
    get { self[TooltipPresentationDelayKey.self] }
    set { self[TooltipPresentationDelayKey.self] = newValue }
  }
}

private struct TooltipPresentationDelayKey: EnvironmentKey {
  static let defaultValue = TooltipTiming.presentationDelay
}
