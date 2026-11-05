cask "quotaline" do
  version "0.2.2"
  sha256 "4a0c7a028e8b01a7fc10af4f61e2294636ef412698dba5610d3c9362344b8dd6"

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
