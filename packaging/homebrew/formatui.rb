class Formatui < Formula
  desc "Rename files with ordered regex rules, from a terminal UI or the command line"
  homepage "https://github.com/simoncoulton/formatui"
  url "https://github.com/simoncoulton/formatui/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_SHA256_OF_THE_TARBALL"
  license "MIT"
  head "https://github.com/simoncoulton/formatui.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  def caveats
    <<~EOS
      Rules live in ~/.config/formatui/config.yaml (created on first run).
      Try it without touching anything:
        formatui run .          # dry run
        formatui                # interactive UI
    EOS
  end

  test do
    ENV["HOME"] = testpath
    assert_match version.to_s, shell_output("#{bin}/formatui --version")
    assert_match "Example Show - S01E02.mkv",
                 shell_output("#{bin}/formatui test Example.Show.2024.S01E02.1080p.mkv")
  end
end
