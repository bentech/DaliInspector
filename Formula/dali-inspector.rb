class DaliInspector < Formula
  desc "DALI USB HID logger and control gear inspector"
  homepage "https://github.com/bentech/DaliInspector"
  url "git@github.com:bentech/DaliInspector.git", using: :git, tag: "v1.0.0", revision: "a857b84e695a854b3dbac5c49dccb7ca38e6e760"

  depends_on "rust" => :build

  deny_network_access!

  def fetch
    system "cargo", "fetch", *std_cargo_fetch_args
  end

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "dali-inspector", shell_output("#{bin}/dali-inspector --help")
  end
end
