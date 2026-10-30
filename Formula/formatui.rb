class Formatui < Formula
  desc "Rename files with ordered regex rules, from a terminal UI or the command line"
  homepage "https://github.com/simoncoulton/formatui"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/simoncoulton/formatui/releases/download/v0.1.0/formatui-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/simoncoulton/formatui/releases/download/v0.1.0/formatui-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/simoncoulton/formatui/releases/download/v0.1.0/formatui-v0.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/simoncoulton/formatui/releases/download/v0.1.0/formatui-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "formatui"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/formatui --version")
  end
end
