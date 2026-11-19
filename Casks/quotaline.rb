cask "quotaline" do
  version "0.3.0"
  sha256 "28c49b9ab6fc22e0582337cde2c28bfc2bf96662268f48f72a50369aaab894d9"

  url "https://github.com/quotaline/quotaline/releases/download/v#{version}/Quotaline-Homebrew.dmg"
  name "Quotaline"
  desc "Menu bar monitor for Claude, Codex, Gemini, Antigravity, Cursor and Copilot usage limits"
  homepage "https://github.com/quotaline/quotaline"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: :sequoia

  app "Quotaline.app"
  uninstall quit: "tech.bernat.quotaline"

  zap trash: [
    "~/Library/Application Support/Quotaline",
    "~/Library/Caches/tech.bernat.quotaline",
    "~/Library/Group Containers/*.tech.bernat.quotaline",
    "~/Library/Group Containers/group.tech.bernat.quotaline",
    "~/Library/HTTPStorages/tech.bernat.quotaline",
    "~/Library/Preferences/tech.bernat.quotaline.plist",
    "~/Library/Saved Application State/tech.bernat.quotaline.savedState",
  ]
end
