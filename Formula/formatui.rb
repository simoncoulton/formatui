class Formatui < Formula
  desc "Rename files with ordered regex rules, from a terminal UI or the command line"
  homepage "https://github.com/simoncoulton/formatui"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/simoncoulton/formatui/releases/download/v0.1.0/formatui-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "ca620f35b38cccbdbf7583702ee3f043dbed74cc3c8c2e8674625a772de07900"
    end
    on_intel do
      url "https://github.com/simoncoulton/formatui/releases/download/v0.1.0/formatui-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "5f8caf56317c02c6acbd31db602da51e3c321f6c629dc78432f6b8bd80b18010"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/simoncoulton/formatui/releases/download/v0.1.0/formatui-v0.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "e61502c0e974d861ad28cf47b539a8061341ddb08a691c0dc579e6d31019f3e0"
    end
    on_intel do
      url "https://github.com/simoncoulton/formatui/releases/download/v0.1.0/formatui-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "bb3df37fa066ef560335824afdae6e1730a18664b9db06a7110178b5af2ae5d3"
    end
  end

  def install
    bin.install "formatui"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/formatui --version")
  end
end
