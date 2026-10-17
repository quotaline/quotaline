import Foundation

public struct HSBColor: Hashable, Sendable {
  public let hue: Double
  public let saturation: Double
  public let brightness: Double

  public init(hue: Double, saturation: Double, brightness: Double) {
    self.hue = hue
    self.saturation = saturation
    self.brightness = brightness
  }

  func mixed(with other: HSBColor, fraction: Double) -> HSBColor {
    HSBColor(
      hue: hue + (other.hue - hue) * fraction,
      saturation: saturation + (other.saturation - saturation) * fraction,
      brightness: brightness + (other.brightness - brightness) * fraction)
  }
}

public enum UsageContrast: Sendable, Hashable {
  case standard
  case increased

  public init(increased: Bool) {
    self = increased ? .increased : .standard
  }
}

public struct UsageLadder: Hashable, Sendable {
  public let green: HSBColor
  public let orange: HSBColor
  public let red: HSBColor

  private init(green: HSBColor, orange: HSBColor, red: HSBColor) {
    self.green = green
    self.orange = orange
    self.red = red
  }

  public static let standard = UsageLadder(
    green: HSBColor(hue: 0.38, saturation: 0.72, brightness: 0.52),
    orange: HSBColor(hue: 0.08, saturation: 0.9, brightness: 0.86),
    red: HSBColor(hue: 0, saturation: 0.85, brightness: 0.8))
  public static let increased = UsageLadder(
    green: HSBColor(hue: 0.38, saturation: 0.95, brightness: 0.4),
    orange: HSBColor(hue: 0.08, saturation: 1, brightness: 0.74),
    red: HSBColor(hue: 0, saturation: 1, brightness: 0.6))
  public static let status = UsageLadder(
    green: HSBColor(hue: 0.38, saturation: 0.7, brightness: 1),
    orange: HSBColor(hue: 0.08, saturation: 0.78, brightness: 1),
    red: HSBColor(hue: 0.025, saturation: 0.55, brightness: 1))
  public static let lightStatus = UsageLadder(
    green: HSBColor(hue: 0.401, saturation: 0.671, brightness: 0.24),
    orange: HSBColor(hue: 0.063, saturation: 0.676, brightness: 0.336),
    red: HSBColor(hue: 0.026, saturation: 0.654, brightness: 0.353))

  public static func ladder(for contrast: UsageContrast) -> UsageLadder {
    contrast == .increased ? increased : standard
  }
}

public enum UsageColor {
  public static let green = UsageLadder.standard.green
  public static let orange = UsageLadder.standard.orange
  public static let red = UsageLadder.standard.red
  public static let orangeAt = 0.6

  public static func color(percent: Double, contrast: UsageContrast = .standard) -> HSBColor {
    color(pace: .unknown, percent: percent, contrast: contrast)
  }

  public static func color(pace: PaceStatus, percent: Double, contrast: UsageContrast = .standard) -> HSBColor {
    color(pace: pace, percent: percent, ladder: UsageLadder.ladder(for: contrast))
  }

  public static func statusColor(percent: Double, dark: Bool) -> HSBColor {
    color(pace: .unknown, percent: percent, ladder: dark ? .status : .lightStatus)
  }

  private static func color(pace: PaceStatus, percent: Double, ladder: UsageLadder) -> HSBColor {
    switch pace {
    case .ahead: return ladder.orange
    case .exhausted: return ladder.red
    case .unknown, .onTrack, .behind:
      let fraction = min(max(percent, 0), 100) / 100
      if fraction == 1 { return ladder.red }
      if fraction < orangeAt {
        return ladder.green.mixed(with: ladder.orange, fraction: pow(fraction / orangeAt, 2))
      }
      return ladder.orange.mixed(with: ladder.red, fraction: (fraction - orangeAt) / (1 - orangeAt))
    }
  }
}
