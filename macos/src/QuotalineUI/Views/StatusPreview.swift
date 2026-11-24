import AppKit
import QuotalineCore
import SwiftUI

public struct StatusPreview: View {
  public let model: StatusItemModel
  @Binding private var highlightedKey: WindowKey?
  private let select: (WindowKey) -> Void
  @Environment(\.colorScheme) private var colorScheme

  public init(model: StatusItemModel, highlightedKey: Binding<WindowKey?>, select: @escaping (WindowKey) -> Void) {
    self.model = model
    _highlightedKey = highlightedKey
    self.select = select
  }

  public var body: some View {
    Group {
      if !model.showsIcon {
        WrappingHStack(horizontalSpacing: 6, verticalSpacing: 4) {
          ForEach(model.cells) { cell in preview(cell) }
        }
      } else {
        HStack(spacing: 0) {
          Image(nsImage: StatusItemRenderer.previewImage(for: model, height: 24, dark: colorScheme == .dark))
            .accessibilityValue(StatusItemRenderer.accessibilityDescription(for: model))
          Spacer(minLength: 0)
        }
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Menu bar preview")
    .accessibilityIdentifier("menu-bar-preview")
    .padding(.horizontal, 6)
    .padding(.vertical, 2)
    .background(
      colorScheme == .dark ? Color.black.opacity(0.5) : Color.white.opacity(0.7),
      in: RoundedRectangle(cornerRadius: 6)
    )
  }

  @ViewBuilder
  func preview(_ cell: StatusCell) -> some View {
    if let key = WindowKey(storageKey: cell.id) {
      let help =
        "Shows the status cell from the same model used by the menu bar. "
        + "Select it to scroll to and focus its model row."
      let tooltip = TooltipContent(title: "Preview \(cell.tooltip)", body: help)
      StatusPreviewCellButton(
        image: StatusItemRenderer.cellImage(cell, height: 24, dark: colorScheme == .dark),
        accessibilityIdentifier: "status-preview-\(cell.id)",
        accessibilityLabel: cell.tooltip,
        accessibilityHelp: help,
        tooltip: tooltip,
        selected: highlightedKey == key
      ) {
        highlightedKey = key
        select(key)
      }
      .richHelpAccessibility(tooltip)
      .hoverTracking { inside in
        if inside { highlightedKey = key } else if highlightedKey == key { highlightedKey = nil }
      }
    } else {
      Image(nsImage: StatusItemRenderer.cellImage(cell, height: 24, dark: colorScheme == .dark))
        .accessibilityLabel(cell.tooltip)
    }
  }
}

private struct StatusPreviewCellButton: NSViewRepresentable {
  let image: NSImage
  let accessibilityIdentifier: String
  let accessibilityLabel: String
  let accessibilityHelp: String
  let tooltip: TooltipContent
  let selected: Bool
  let action: () -> Void

  func makeNSView(context: Context) -> NativeButtonContainer {
    let button = StatusPreviewButton()
    button.cell = StatusPreviewButtonCell(imageCell: nil)
    button.target = context.coordinator
    button.action = #selector(Coordinator.press(_:))
    button.setButtonType(.toggle)
    button.bezelStyle = .regularSquare
    button.controlSize = .small
    button.imagePosition = .imageOnly
    button.imageScaling = .scaleNone
    button.refusesFirstResponder = false
    button.setAccessibilityElement(true)
    button.setAccessibilityRole(.button)
    context.coordinator.tooltip.autoresizingMask = [.width, .height]
    button.addSubview(context.coordinator.tooltip)
    update(button, coordinator: context.coordinator)
    return NativeButtonContainer(button: button)
  }

  func updateNSView(_ container: NativeButtonContainer, context: Context) {
    context.coordinator.tooltip.tooltipPresentationDelay = context.environment.tooltipPresentationDelay
    context.coordinator.tooltip.update(content: tooltip, focused: false)
    update(container.button, coordinator: context.coordinator)
  }

  func sizeThatFits(_ proposal: ProposedViewSize, nsView: NativeButtonContainer, context: Context) -> CGSize? {
    nsView.button.cell!.cellSize
  }

  func makeCoordinator() -> Coordinator {
    Coordinator(action: action, tooltip: tooltip)
  }

  static func dismantleNSView(_ container: NativeButtonContainer, coordinator: Coordinator) {
    coordinator.tooltip.dismantle()
  }

  private func update(_ button: NSButton, coordinator: Coordinator) {
    coordinator.action = action
    button.image = image
    button.state = selected ? .on : .off
    button.setAccessibilityIdentifier(accessibilityIdentifier)
    button.setAccessibilityLabel(accessibilityLabel)
    button.setAccessibilityHelp(accessibilityHelp)
  }

  @MainActor
  final class Coordinator: NSObject {
    var action: () -> Void
    let tooltip: TooltipTrackingView

    init(action: @escaping () -> Void, tooltip: TooltipContent) {
      self.action = action
      self.tooltip = TooltipTrackingView(content: tooltip, presenter: .shared)
    }

    @objc func press(_: NSButton) {
      action()
    }
  }
}

private final class StatusPreviewButtonCell: FilledButtonCell {
  private static let padding = NSSize(width: 6, height: 2)

  override var cellSize: NSSize {
    let image = image!.size
    return NSSize(width: image.width + 2 * Self.padding.width, height: image.height + 2 * Self.padding.height)
  }

  override func imageRect(forBounds rect: NSRect) -> NSRect {
    let image = image!.size
    return NSRect(
      x: rect.midX - image.width / 2, y: rect.midY - image.height / 2, width: image.width, height: image.height)
  }
}

private final class StatusPreviewButton: NSButton {
  override func accessibilityPerformPress() -> Bool {
    performClick(nil)
    return true
  }
}
