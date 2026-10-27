cask "quotaline" do
  version "0.2.0"
  sha256 "d729a91850d21a8e4f62c0f4d938a687bc7f99e28ab3aa2d66bd3c6b8b5dc811"

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
