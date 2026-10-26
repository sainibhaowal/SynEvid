# typed: false
# frozen_string_literal: true

# Homebrew Formula for SynEvid (Code Autopsy)
# Install via tap:
#   brew tap sainibhaowal/synevid https://github.com/sainibhaowal/SynEvid
#   brew install synevid
# Or directly:
#   brew install --formula Formula/synevid.rb

class Synevid < Formula
  desc "Offline, deterministic software verification, blast-radius, and agent forensic engine"
  homepage "https://github.com/sainibhaowal/SynEvid"
  version "0.1.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/sainibhaowal/SynEvid/releases/download/v0.1.0/autopsy-aarch64-apple-darwin.tar.gz"
      sha256 "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    else
      url "https://github.com/sainibhaowal/SynEvid/releases/download/v0.1.0/autopsy-x86_64-apple-darwin.tar.gz"
      sha256 "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/sainibhaowal/SynEvid/releases/download/v0.1.0/autopsy-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    else
      url "https://github.com/sainibhaowal/SynEvid/releases/download/v0.1.0/autopsy-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    end
  end

  def install
    bin.install "autopsy"
    bin.install "synevid-lsp" if File.exist?("synevid-lsp")
    bin.install_symlink bin/"autopsy" => "synevid"
    bin.install_symlink bin/"autopsy" => "synevid-ui"
    (pkgshare/"mcp-server").install "mcp-server/index.js" if File.exist?("mcp-server/index.js")
    pkgshare.install "setup-agent-hosts.py" if File.exist?("setup-agent-hosts.py")
    (pkgshare/"hooks").install Dir["hooks/*.sh"] if Dir.exist?("hooks")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/autopsy version")
    assert_match "Usage:", shell_output("#{bin}/autopsy --help")
  end
end
