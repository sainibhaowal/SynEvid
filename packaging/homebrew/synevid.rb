# Homebrew Formula for SynEvid (Code Autopsy)
# Install via tap: brew install sainibhaowal/tap/synevid
# Or directly:     brew install --formula packaging/homebrew/synevid.rb

class Synevid < Formula
  desc "Offline, deterministic software verification, blast-radius, and agent forensic engine"
  homepage "https://github.com/sainibhaowal/SynEvid"
  version "0.1.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/sainibhaowal/SynEvid/releases/download/v0.1.0/autopsy-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_AARCH64_APPLE_DARWIN"
    else
      url "https://github.com/sainibhaowal/SynEvid/releases/download/v0.1.0/autopsy-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_X86_64_APPLE_DARWIN"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/sainibhaowal/SynEvid/releases/download/v0.1.0/autopsy-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256_AARCH64_UNKNOWN_LINUX_GNU"
    else
      url "https://github.com/sainibhaowal/SynEvid/releases/download/v0.1.0/autopsy-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256_X86_64_UNKNOWN_LINUX_GNU"
    end
  end

  def install
    bin.install "autopsy"
    bin.install "synevid-lsp" if File.exist?("synevid-lsp")
    (pkgshare/"mcp-server").install "mcp-server/index.js" if File.exist?("mcp-server/index.js")
    pkgshare.install "setup-agent-hosts.py" if File.exist?("setup-agent-hosts.py")
    (pkgshare/"hooks").install Dir["hooks/*.sh"] if Dir.exist?("hooks")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/autopsy version")
  end
end
