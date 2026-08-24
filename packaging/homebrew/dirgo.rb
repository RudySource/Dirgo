class Dirgo < Formula
  desc "Instant directory navigation for your terminal"
  homepage "https://github.com/RudySource/Dirgo"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/RudySource/Dirgo/releases/download/v0.2.0/dirgo-v0.2.0-aarch64-apple-darwin.tar.gz"
      sha256 "43c1ceaae1d421e71db2d367af23dfce5fd86a0ed4d46a47d887a3dd8b987bd3"
    end

    on_intel do
      url "https://github.com/RudySource/Dirgo/releases/download/v0.2.0/dirgo-v0.2.0-x86_64-apple-darwin.tar.gz"
      sha256 "38bbb0f0a57acde42e787e7d7d29c9dccc8c995d778a5223b55bd2f6938b023e"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/RudySource/Dirgo/releases/download/v0.2.0/dirgo-v0.2.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e19ceaf3dbd382b14fbfd6ea55fd400de30cfc3f7ce8098cb3b96847101f4bbb"
    end
  end

  def install
    bin.install "dgo"
  end

  test do
    assert_match "dgo #{version}", shell_output("#{bin}/dgo --version")
  end
end
