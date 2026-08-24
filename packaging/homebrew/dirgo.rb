class Dirgo < Formula
  desc "Instant directory navigation for your terminal"
  homepage "https://github.com/RudySource/Dirgo"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/RudySource/Dirgo/releases/download/v0.1.3/dirgo-v0.1.3-aarch64-apple-darwin.tar.gz"
      sha256 "49a02d001fe9f628d16e9d1fb544ddc2ce9852dbd0e3dec0af47d90e91a0442c"
    end

    on_intel do
      url "https://github.com/RudySource/Dirgo/releases/download/v0.1.3/dirgo-v0.1.3-x86_64-apple-darwin.tar.gz"
      sha256 "e00619f1d1f0a234c7e9d071f43a8807efad64e843da9ffad6c96c618d910332"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/RudySource/Dirgo/releases/download/v0.1.3/dirgo-v0.1.3-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "5127e8ffde3d5c2d860681138f944e9e1d25a0b36b36b840f758626b964eba27"
    end
  end

  def install
    bin.install "dgo"
  end

  test do
    assert_match "dgo #{version}", shell_output("#{bin}/dgo --version")
  end
end
