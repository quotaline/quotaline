import AppKit
import QuotalineCore
import Testing

@Test func increasedContrastLadderIsMoreSaturatedAndDarkerAtEveryStop() {
  let pairs = [
    (UsageLadder.standard.green, UsageLadder.increased.green),
    (UsageLadder.standard.orange, UsageLadder.increased.orange),
    (UsageLadder.standard.red, UsageLadder.increased.red),
  ]
  for (standard, increased) in pairs {
    #expect(increased.hue == standard.hue)
    #expect(increased.saturation > standard.saturation)
    #expect(increased.brightness < standard.brightness)
  }
}

@Test func increasedContrastColoursFollowTheSameLadderShape() {
  #expect(UsageColor.color(percent: 0, contrast: .increased) == UsageLadder.increased.green)
  #expect(UsageColor.color(percent: 60, contrast: .increased) == UsageLadder.increased.orange)
  #expect(UsageColor.color(percent: 100, contrast: .increased) == UsageLadder.increased.red)
  #expect(UsageColor.color(percent: 30, contrast: .increased) != UsageColor.color(percent: 30))
  #expect(UsageColor.color(pace: .ahead, percent: 10, contrast: .increased) == UsageLadder.increased.orange)
  #expect(UsageColor.color(pace: .exhausted, percent: 10, contrast: .increased) == UsageLadder.increased.red)
  #expect(UsageColor.color(pace: .onTrack, percent: 0, contrast: .increased) == UsageLadder.increased.green)
}

@Test(arguments: [(false, UsageContrast.standard), (true, .increased)])
func usageContrastMapsTheAccessibilityFlag(increased: Bool, expected: UsageContrast) {
  #expect(UsageContrast(increased: increased) == expected)
  #expect(UsageLadder.ladder(for: expected) == (increased ? UsageLadder.increased : UsageLadder.standard))
}

@Test func statusColoursFollowTheAccessibleLadder() {
  #expect(UsageColor.statusColor(percent: 0, dark: true) == UsageLadder.status.green)
  #expect(UsageColor.statusColor(percent: 60, dark: true) == UsageLadder.status.orange)
  #expect(UsageColor.statusColor(percent: 100, dark: true) == UsageLadder.status.red)
  #expect(UsageColor.statusColor(percent: 0, dark: false) == UsageLadder.lightStatus.green)
  #expect(UsageColor.statusColor(percent: 60, dark: false) == UsageLadder.lightStatus.orange)
  #expect(UsageColor.statusColor(percent: 100, dark: false) == UsageLadder.lightStatus.red)
}

@Test func lightStatusColoursRemainReadableOnSilver() {
  let linear: (CGFloat) -> Double = { value in
    let channel = Double(value)
    return channel <= 0.04045 ? channel / 12.92 : pow((channel + 0.055) / 1.055, 2.4)
  }
  let background = linear(241 / 255)
  for percent in 0...100 {
    let tone = UsageColor.statusColor(percent: Double(percent), dark: false)
    let color = NSColor(
      hue: tone.hue, saturation: tone.saturation, brightness: tone.brightness, alpha: 1
    ).usingColorSpace(.deviceRGB)!
    let luminance =
      0.2126 * linear(color.redComponent) + 0.7152 * linear(color.greenComponent)
      + 0.0722 * linear(color.blueComponent)
    #expect((background + 0.05) / (luminance + 0.05) >= 7, "\(percent)%")
  }
}
