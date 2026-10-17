import AppKit
import QuotalineCore

public struct StatusRenderSignature: Hashable, Sendable {
  public let model: StatusItemModel
  public let dark: Bool
  public let height: Double

  public init(model: StatusItemModel, dark: Bool, height: Double) {
    self.model = model
    self.dark = dark
    self.height = height
  }
}

@MainActor
public enum StatusItemRenderer {
  public static let maxFontSize: CGFloat = 13
  public static let minFontSize: CGFloat = 8
  public static let cellPadding: CGFloat = 6
  public static let separatorWidth: CGFloat = 8
  public static let barWidth: CGFloat = 30
  public static let barHeight: CGFloat = 4

  public static func fontSizes(height: CGFloat, lineCount: Int) -> [CGFloat] {
    StatusMetrics.fontSizes(height: Double(height), lineCount: lineCount).map { CGFloat($0) }
  }

  public static func color(for kind: StatusRun.Kind, dark: Bool, isStale: Bool = false) -> NSColor {
    if isStale, kind != .label { return NSColor(calibratedWhite: dark ? 0.7 : 0.35, alpha: 1) }
    switch kind {
    case .label:
      return dark ? NSColor.white : NSColor.black
    case .number:
      return (dark ? NSColor.white : NSColor.black).withAlphaComponent(0.85)
    case .usage(let percent):
      let hsb = UsageColor.statusColor(percent: percent, dark: dark)
      return NSColor(
        hue: hsb.hue, saturation: hsb.saturation, brightness: hsb.brightness, alpha: 1)
    }
  }

  private struct CachedTitle {
    let signature: StatusRenderSignature
    let title: NSAttributedString
  }

  /// Keep one title per possible width tier so measuring the ladder and applying any tier share the same build.
  private static var titleCache: [CachedTitle] = []

  public static func attributedTitle(for model: StatusItemModel, height: CGFloat, dark: Bool) -> NSAttributedString {
    let signature = StatusRenderSignature(model: model, dark: dark, height: Double(height))
    if let index = titleCache.firstIndex(where: { $0.signature == signature }) {
      let cached = titleCache.remove(at: index)
      titleCache.append(cached)
      return cached.title
    }
    let title = build(model, height: height, dark: dark)
    if titleCache.count == StatusTier.allCases.count { titleCache.removeFirst() }
    titleCache.append(CachedTitle(signature: signature, title: title))
    return title
  }

  /// The item draws its content into images, so VoiceOver needs both the visible rendering and its full context.
  public static func accessibilityDescription(for model: StatusItemModel) -> String {
    if let summary = model.accessibilitySummary { return "Quotaline, " + summary }
    let readings = model.cells.flatMap { cell in
      var descriptions: [String] = []
      if cell.isMiniBar {
        let bars = cell.bars.map { "\($0.label) bar at \(Format.percent($0.percent))" }.joined(separator: ", ")
        if !bars.isEmpty { descriptions.append("Displayed \(bars)") }
      } else {
        let rendered = cell.lines.map { $0.map(\.text).joined() }.joined(separator: " / ")
        if !rendered.isEmpty { descriptions.append("Displayed \(rendered)") }
      }
      descriptions += cell.tooltip.split(separator: "\n").map(String.init)
      return descriptions
    }
    return readings.isEmpty ? "Quotaline, no usage yet" : "Quotaline, " + readings.joined(separator: ", ")
  }

  private static func build(_ model: StatusItemModel, height: CGFloat, dark: Bool) -> NSAttributedString {
    let title = NSMutableAttributedString()
    for (index, cell) in model.cells.enumerated() {
      if index > 0 { title.append(attachment(separatorImage(height: height))) }
      title.append(attachment(cellImage(cell, height: height, dark: dark)))
    }
    return title
  }

  static func attachment(_ image: NSImage) -> NSAttributedString {
    let attachment = NSTextAttachment()
    attachment.image = image
    let font = NSFont.menuBarFont(ofSize: 0)
    attachment.bounds = CGRect(
      x: 0, y: (font.capHeight - image.size.height) / 2, width: image.size.width, height: image.size.height)
    return NSAttributedString(attachment: attachment)
  }

  static func separatorImage(height: CGFloat) -> NSImage {
    NSImage(size: CGSize(width: separatorWidth, height: height), flipped: false) { rect in
      NSColor.tertiaryLabelColor.withAlphaComponent(0.5).setFill()
      NSRect(x: rect.midX - 0.5, y: 4, width: 1, height: rect.height - 8).fill()
      return true
    }
  }

  public static func cellImage(_ cell: StatusCell, height: CGFloat, dark: Bool) -> NSImage {
    cell.isMiniBar ? miniBarImage(cell, height: height, dark: dark) : textImage(cell, height: height, dark: dark)
  }

  static func textImage(_ cell: StatusCell, height: CGFloat, dark: Bool) -> NSImage {
    let lines = lineStrings(cell, height: height, dark: dark)
    // Laying out an attributed string is the expensive part here, and AppKit calls the drawing handler again on
    // every scale and appearance change, so measure once and carry the sizes in.
    let sizes = lines.map { $0.size() }
    let width = ceil(sizes.map(\.width).max() ?? 0) + cellPadding * 2
    let total = sizes.map(\.height).reduce(0, +)
    return NSImage(size: CGSize(width: width, height: height), flipped: true) { rect in
      var lineTop = (rect.height - total) / 2
      for (line, size) in zip(lines, sizes) {
        line.draw(at: CGPoint(x: (rect.width - size.width) / 2, y: lineTop))
        lineTop += size.height
      }
      return true
    }
  }

  static func lineStrings(_ cell: StatusCell, height: CGFloat, dark: Bool) -> [NSAttributedString] {
    zip(cell.lines, fontSizes(height: height, lineCount: cell.lines.count)).map { runs, size in
      let line = NSMutableAttributedString()
      for run in runs {
        let font: NSFont =
          switch run.kind {
          case .label: NSFont.systemFont(ofSize: size, weight: .regular)
          default: NSFont.monospacedDigitSystemFont(ofSize: size, weight: .medium)
          }
        line.append(
          NSAttributedString(
            string: run.text,
            attributes: [.font: font, .foregroundColor: color(for: run.kind, dark: dark, isStale: cell.isStale)]))
      }
      return line
    }
  }

  static func miniBarImage(_ cell: StatusCell, height: CGFloat, dark: Bool) -> NSImage {
    let glyph = ProviderGlyph.image(cell.provider, pointSize: min(height * 0.55, 12))
    let labelFont = NSFont.systemFont(ofSize: 8, weight: .bold)
    let labels = cell.bars.map {
      NSAttributedString(
        string: $0.label, attributes: [.font: labelFont, .foregroundColor: color(for: .label, dark: dark)])
    }
    let labelSizes = labels.map { $0.size() }
    let labelWidth = labelSizes.map(\.width).max() ?? 0
    let width = cellPadding + glyph.size.width + 4 + ceil(labelWidth) + 3 + barWidth + cellPadding
    let bars = cell.bars
    let trackColor = color(for: .label, dark: dark).withAlphaComponent(0.18)
    let barColors = bars.map { color(for: .usage($0.percent), dark: dark, isStale: cell.isStale) }
    return NSImage(size: CGSize(width: width, height: height), flipped: true) { rect in
      let glyphY = (rect.height - glyph.size.height) / 2
      glyph.draw(
        in: CGRect(x: cellPadding, y: glyphY, width: glyph.size.width, height: glyph.size.height), from: .zero,
        operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
      let rowHeight = rect.height / CGFloat(max(bars.count, 1))
      let labelLeft = cellPadding + glyph.size.width + 4
      for (index, bar) in bars.enumerated() {
        let centerY = rowHeight * CGFloat(index) + rowHeight / 2
        labels[index].draw(at: CGPoint(x: labelLeft, y: centerY - labelSizes[index].height / 2))
        let trackRect = CGRect(
          x: labelLeft + ceil(labelWidth) + 3, y: centerY - barHeight / 2, width: barWidth,
          height: barHeight)
        trackColor.setFill()
        NSBezierPath(roundedRect: trackRect, xRadius: barHeight / 2, yRadius: barHeight / 2).fill()
        let filled = max(barWidth * CGFloat(min(max(bar.percent, 0), 100) / 100), 2.5)
        barColors[index].setFill()
        NSBezierPath(
          roundedRect: CGRect(x: trackRect.minX, y: trackRect.minY, width: filled, height: barHeight),
          xRadius: barHeight / 2, yRadius: barHeight / 2
        ).fill()
      }
      return true
    }
  }

  /// Renders the status item on a menu-bar-like strip. The docs use this rather than a screen capture, which would
  /// carry whatever else crowds the machine's own menu bar.
  public static func stripImage(
    for model: StatusItemModel, height: CGFloat = 24, dark: Bool, width: CGFloat = 520
  )
    -> NSImage
  {
    let cells = previewImage(for: model, height: height, dark: dark)
    return NSImage(size: CGSize(width: width, height: height + 4), flipped: false) { rect in
      let background =
        dark
        ? NSColor(calibratedWhite: 0.13, alpha: 1)
        : NSColor(calibratedWhite: 0.93, alpha: 1)
      background.setFill()
      NSBezierPath(roundedRect: rect, xRadius: 6, yRadius: 6).fill()
      cells.draw(
        at: CGPoint(x: rect.maxX - cells.size.width - 12, y: (rect.height - height) / 2), from: .zero,
        operation: .sourceOver, fraction: 1)
      return true
    }
  }

  /// Renders at a device scale so the strip stays sharp when the website shows it a few hundred points wide.
  public static func stripData(for model: StatusItemModel, dark: Bool, scale: Int = 3) -> Data {
    let image = stripImage(for: model, dark: dark)
    let pixels = CGSize(width: image.size.width * CGFloat(scale), height: image.size.height * CGFloat(scale))
    // The strip has a fixed, positive size, so its RGBA bitmap always allocates and encodes.
    let rep = NSBitmapImageRep(
      bitmapDataPlanes: nil, pixelsWide: Int(pixels.width), pixelsHigh: Int(pixels.height), bitsPerSample: 8,
      samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0,
      bitsPerPixel: 0)!
    rep.size = image.size
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    image.draw(in: CGRect(origin: .zero, size: image.size))
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
  }

  public static func previewImage(for model: StatusItemModel, height: CGFloat, dark: Bool) -> NSImage {
    let images = model.cells.enumerated().flatMap { index, cell -> [NSImage] in
      let image = cellImage(cell, height: height, dark: dark)
      return index == 0 ? [image] : [separatorImage(height: height), image]
    }
    let icon = model.showsIcon ? [AppIcon.image(height: height, tone: model.iconTone, dark: dark)] : []
    let all = icon + images
    let width = max(all.map(\.size.width).reduce(0, +), 1)
    return NSImage(size: CGSize(width: width, height: height), flipped: false) { _ in
      var x: CGFloat = 0
      for image in all {
        image.draw(at: CGPoint(x: x, y: 0), from: .zero, operation: .sourceOver, fraction: 1)
        x += image.size.width
      }
      return true
    }
  }
}
