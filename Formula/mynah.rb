# Homebrew formula. Builds Mynah from source (Rust + Node toolchains come from Homebrew,
# crates and npm packages are fetched during the build; expect 10-20 minutes).
#
#   brew tap agiangrant/mynah https://github.com/agiangrant/mynah
#   brew install agiangrant/mynah/mynah          # latest tagged release
#   brew install --HEAD agiangrant/mynah/mynah   # main branch
class Mynah < Formula
  desc "Local speech-to-text for the desktop: hold a key, talk, text appears"
  homepage "https://github.com/agiangrant/mynah"
  url "https://github.com/agiangrant/mynah/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "SHA256_PLACEHOLDER"
  license "MIT"
  head "https://github.com/agiangrant/mynah.git", branch: "main"

  depends_on "cmake" => :build
  depends_on "node" => :build
  depends_on "pnpm" => :build
  depends_on "rust" => :build
  depends_on :macos

  def install
    ENV["CI"] = "true"
    # cmake (whisper.cpp) must use the same SDK rustc links against; the Command Line
    # Tools SDK can be newer than the toolchain's linker and then fails with tapi errors.
    sdk = Utils.safe_popen_read("/usr/bin/xcrun", "--sdk", "macosx", "--show-sdk-path").strip
    ENV["SDKROOT"] = sdk unless sdk.empty?

    system "pnpm", "install", "--frozen-lockfile"
    system "pnpm", "tauri", "build", "--bundles", "app", "--ci"

    prefix.install "src-tauri/target/release/bundle/macos/Mynah.app"
    libexec.install "scripts/make-signing-cert.sh", "scripts/sign-macos.sh", "scripts/mynah-launcher.sh",
                    "src-tauri/entitlements.plist"
    inreplace libexec/"mynah-launcher.sh" do |s|
      s.gsub! "@APP@", "#{opt_prefix}/Mynah.app"
      s.gsub! "@LIBEXEC@", opt_libexec.to_s
    end
    chmod 0755, libexec/"mynah-launcher.sh"
    bin.install_symlink libexec/"mynah-launcher.sh" => "mynah"
  end

  def caveats
    <<~EOS
      Mynah is an app bundle:
        #{opt_prefix}/Mynah.app
      Start it with `mynah`, or make it show up in Spotlight and Launchpad:
        ln -sfn "#{opt_prefix}/Mynah.app" /Applications/Mynah.app

      macOS ties Accessibility / Input Monitoring / Microphone grants to the app's code
      signature, and an unsigned build gets a new one every upgrade. Create a local
      signing identity once (you will be asked for your login password) and the grants
      stick from then on:
        mynah setup-signing
    EOS
  end

  test do
    assert_predicate prefix/"Mynah.app/Contents/MacOS/mynah", :executable?
    assert_match "Mynah.app", shell_output("#{bin}/mynah app-path")
  end
end
