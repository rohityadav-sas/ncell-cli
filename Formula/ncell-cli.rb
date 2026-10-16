class NcellCli < Formula
  desc "Ncell Nepal CLI for OTP login, balances, plans and free SMS"
  homepage "https://rohityadav-sas.github.io/ncell-cli/"
  url "https://github.com/rohityadav-sas/ncell-cli/archive/refs/tags/v9.9.11.tar.gz"
  version "9.9.11"
  sha256 "1b99b10be9b92db2e7b441fc9f2a1bbd2135050df05c1629ee4dbf03445d1371"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "ncell-cli 9.9.11", shell_output("#{bin}/ncell --version")
    assert_match "Ncell account CLI", shell_output("#{bin}/ncell --help")
  end
end