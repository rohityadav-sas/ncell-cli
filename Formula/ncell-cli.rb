class NcellCli < Formula
  desc "Ncell Nepal CLI for OTP login, balances, plans and free SMS"
  homepage "https://rohityadav-sas.github.io/ncell-cli/"
  url "https://github.com/rohityadav-sas/ncell-cli/archive/refs/tags/v0.4.3.tar.gz"
  version "0.4.3"
  sha256 "003f225363794fbb2a1a8c1ab74e34b9a70b2fc730b23734e2ef40b177ba457c"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "ncell-cli 0.4.3", shell_output("#{bin}/ncell --version")
    assert_match "Ncell account CLI", shell_output("#{bin}/ncell --help")
  end
end