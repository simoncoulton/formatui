#!/usr/bin/env bash
# Usage: gen-formula.sh <version> <sha-aarch64-apple-darwin> <sha-x86_64-apple-darwin> <sha-aarch64-unknown-linux-gnu> <sha-x86_64-unknown-linux-gnu>
# Writes Formula/formatui.rb pointing at the binaries attached to the GitHub release.
set -euo pipefail
version=$1
base="https://github.com/simoncoulton/formatui/releases/download/v${version}/formatui-v${version}"

cat > "$(dirname "$0")/../Formula/formatui.rb" <<RUBY
class Formatui < Formula
  desc "Rename files with ordered regex rules, from a terminal UI or the command line"
  homepage "https://github.com/simoncoulton/formatui"
  version "${version}"
  license "MIT"

  on_macos do
    on_arm do
      url "${base}-aarch64-apple-darwin.tar.gz"
      sha256 "${2}"
    end
    on_intel do
      url "${base}-x86_64-apple-darwin.tar.gz"
      sha256 "${3}"
    end
  end

  on_linux do
    on_arm do
      url "${base}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "${4}"
    end
    on_intel do
      url "${base}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "${5}"
    end
  end

  def install
    bin.install "formatui"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/formatui --version")
  end
end
RUBY
