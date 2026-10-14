# typed: false
# frozen_string_literal: true

class Droptube < Formula
  desc "Lightweight, touch-friendly local network media streaming server"
  homepage "https://github.com/Bless-04/DropTube-CLI"
  version "0.4.0"

  on_linux do
    if Hardware::CPU.intel? && Hardware::CPU.is_64_bit?
      url "https://github.com/Bless-04/DropTube-CLI/releases/download/v#{version}/droptube-linux-x86_64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Populated automatically by release workflow
    end
  end

  def install
    bin.install "droptube"
  end

  test do
    assert_match "droptube", shell_output("#{bin}/droptube --version")
  end
end
