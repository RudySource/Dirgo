class Dirgo < Formula
  desc "Instant directory navigation for your terminal"
  homepage "https://github.com/RudySource/Dirgo"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/RudySource/Dirgo/releases/download/v0.2.1/dirgo-v0.2.1-aarch64-apple-darwin.tar.gz"
      sha256 "35618f174b880cef743ef7a7211da7764120ae13f735c3896f55018eca12b0b0"
    end

    on_intel do
      url "https://github.com/RudySource/Dirgo/releases/download/v0.2.1/dirgo-v0.2.1-x86_64-apple-darwin.tar.gz"
      sha256 "cfe9c3141d2ae6703c5a63ee6e00b802f8803ea3d43d28f2d99f7887ac217066"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/RudySource/Dirgo/releases/download/v0.2.1/dirgo-v0.2.1-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "51d43b0e46c43303d4f13beeef009198c7e0b7214f3cc6df885f6e5156bfc5d5"
    end
  end

  def install
    bin.install "dgo"
  end

  test do
    assert_match "dgo #{version}", shell_output("#{bin}/dgo --version")
  end
end
