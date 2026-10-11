class NcellCli < Formula
  desc "Ncell Nepal CLI for OTP login, balances, plans and free SMS"
  homepage "https://rohityadav-sas.github.io/ncell-cli/"
  url "https://github.com/rohityadav-sas/ncell-cli/archive/eceb12d.tar.gz"
  version "0.1.0"
  sha256 "94c24e988d1e208619016f257084455733990fb3404b8ccae78d812559d97643"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "ncell 0.1.0", shell_output("#{bin}/ncell --version")
    assert_match "Ncell account CLI", shell_output("#{bin}/ncell --help")
  end
end
