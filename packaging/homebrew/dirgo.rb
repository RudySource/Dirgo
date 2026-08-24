class Dirgo < Formula
  desc "Instant directory navigation for your terminal"
  homepage "https://github.com/RudySource/Dirgo"
  version "0.1.2"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/RudySource/Dirgo/releases/download/v0.1.2/dirgo-v0.1.2-aarch64-apple-darwin.tar.gz"
      sha256 "04607e630a1cf91e657c61a255d7b06c5d9424daf6ef071e1a1003959c73d81c"
    else
      url "https://github.com/RudySource/Dirgo/releases/download/v0.1.2/dirgo-v0.1.2-x86_64-apple-darwin.tar.gz"
      sha256 "a218ccd7b00a83748e60a745c737877f78c6bf30472671399ee174444e0ebb24"
    end
  end

  on_linux do
    url "https://github.com/RudySource/Dirgo/releases/download/v0.1.2/dirgo-v0.1.2-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "ccbe3b846b99d57fd2d70f52dc0ae748b16bf5ecf729aa8f4f84f469a41f93e5"
  end

  def install
    bin.install "dgo"
  end

  test do
    assert_match "dgo #{version}", shell_output("#{bin}/dgo --version")
  end
end
