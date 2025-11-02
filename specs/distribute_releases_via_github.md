# GitHub Actions Release Distribution Plan

## Overview

This document outlines the plan for automating the build and distribution of `mcc-gaql` binaries using GitHub Actions, specifically targeting macOS Apple Silicon (ARM64) platform.

## Current State

### Project Information
- **Repository:** mcc-gaql-rs
- **Current Version:** 0.12.2
- **Binary Name:** `mcc-gaql`
- **Current Branch:** `publish_binaries_via_github_actions`

### Existing CI/CD
- **rust.yml:** Basic CI workflow that builds and tests on Linux (ubuntu-latest)
- **code-review.yml:** AI-powered PR review workflow
- **No formal release process:** No automated binary distribution currently exists
- **Manual git tags:** Some version tags exist (`0.7.0`, `v0.8.0`) but no corresponding releases

### Build Requirements
- **System dependency:** protobuf-compiler (required by tonic/gRPC)
- **Rust toolchain:** Standard cargo build
- **Platform target:** aarch64-apple-darwin (macOS Apple Silicon)

## Proposed Solution

### Release Workflow Configuration

Create a new GitHub Actions workflow file: `.github/workflows/release.yml`

#### Trigger Mechanism
- **Primary trigger:** Git tags matching pattern `v*` (e.g., `v0.13.0`, `v1.0.0`)
- **Rationale:** Follows semantic versioning best practices and provides clear release points

#### Build Platform
- **Runner:** `macos-latest` (currently provides macOS 14 with Apple Silicon support)
- **Target:** `aarch64-apple-darwin` (native Apple Silicon)
- **Rust toolchain:** Stable channel with target pre-installed

#### Build Process
1. **Checkout code:** Use `actions/checkout@v4`
2. **Setup Rust:** Use `dtolnay/rust-toolchain@stable` with aarch64-apple-darwin target
3. **Install dependencies:**
   ```bash
   brew install protobuf
   ```
4. **Build release binary:**
   ```bash
   cargo build --release --target aarch64-apple-darwin
   ```
5. **Locate binary:** `target/aarch64-apple-darwin/release/mcc-gaql`

#### Packaging Strategy
Create a compressed tar.gz archive containing:
- **Binary:** `mcc-gaql` (the compiled executable)
- **Documentation:** `README.md` (usage instructions)
- **License:** `LICENSE` (legal information)

**Archive naming convention:**
```
mcc-gaql-{version}-macos-aarch64.tar.gz
```

Example: `mcc-gaql-0.13.0-macos-aarch64.tar.gz`

#### Release Creation
- **Tool:** GitHub's native release creation (via `softprops/action-gh-release@v1` or similar)
- **Release title:** Use the tag name (e.g., "v0.13.0")
- **Release notes:** Auto-generate from commits since last tag
- **Assets:** Attach the tar.gz archive

### Workflow Implementation Details

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  release-macos-arm64:
    name: Build and Release macOS Apple Silicon
    runs-on: macos-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin

      - name: Install protobuf
        run: brew install protobuf

      - name: Build release binary
        run: cargo build --release --target aarch64-apple-darwin

      - name: Extract version from tag
        id: version
        run: echo "VERSION=${GITHUB_REF#refs/tags/v}" >> $GITHUB_OUTPUT

      - name: Create release archive
        run: |
          mkdir -p release
          cp target/aarch64-apple-darwin/release/mcc-gaql release/
          cp README.md release/
          cp LICENSE release/
          cd release
          tar -czf ../mcc-gaql-${{ steps.version.outputs.VERSION }}-macos-aarch64.tar.gz *

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v1
        with:
          files: mcc-gaql-${{ steps.version.outputs.VERSION }}-macos-aarch64.tar.gz
          generate_release_notes: true
          draft: false
          prerelease: false
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

## Release Process

### For Maintainers

1. **Update version** in `Cargo.toml`
2. **Commit changes:**
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "Bump version to 0.13.0"
   ```
3. **Create and push tag:**
   ```bash
   git tag v0.13.0
   git push origin main
   git push origin v0.13.0
   ```
4. **Automatic workflow:** GitHub Actions will automatically:
   - Detect the new tag
   - Build the macOS ARM64 binary
   - Create release package
   - Publish GitHub Release with downloadable asset

### For Users

Users can download pre-built binaries from:
```
https://github.com/mhuang74/mcc-gaql-rs/releases
```

**Installation steps:**
1. Download `mcc-gaql-{version}-macos-aarch64.tar.gz`
2. Extract: `tar -xzf mcc-gaql-{version}-macos-aarch64.tar.gz`
3. Move binary to PATH: `mv mcc-gaql /usr/local/bin/` (or add to PATH)
4. Make executable: `chmod +x /usr/local/bin/mcc-gaql`
5. Run: `mcc-gaql --help`

## Benefits

1. **Automation:** No manual build/upload steps required
2. **Consistency:** Reproducible builds in clean CI environment
3. **Distribution:** Professional release packages with documentation
4. **User Experience:** Easy download and installation for end users
5. **Versioning:** Clear version tracking via git tags
6. **Transparency:** Public build logs and release history

## Future Enhancements

### Multi-Platform Support
Consider adding support for additional platforms:
- **macOS Intel:** `x86_64-apple-darwin` (older Macs)
- **Linux x86_64:** `x86_64-unknown-linux-gnu` (most common server/desktop)
- **Linux ARM64:** `aarch64-unknown-linux-gnu` (ARM servers, Raspberry Pi)
- **Windows x86_64:** `x86_64-pc-windows-msvc` (Windows users)

### Advanced Features
- **Checksums:** Generate SHA256 checksums for downloads
- **Code signing:** Sign macOS binaries for enhanced security
- **Homebrew tap:** Create formulae for easy installation via `brew install`
- **Docker images:** Publish container images for cloud deployments
- **Version management:** Integrate `cargo-release` for version bumping
- **Changelog:** Auto-generate CHANGELOG.md from commit history

### Testing
- **Pre-release testing:** Run integration tests before creating release
- **Binary verification:** Test that built binary actually runs
- **Cross-compilation testing:** Verify builds on multiple platforms

## Implementation Checklist

- [ ] Create `.github/workflows/release.yml` workflow file
- [ ] Test workflow with a test tag (e.g., `v0.12.3-test`)
- [ ] Verify binary builds successfully
- [ ] Confirm release package contains all files
- [ ] Validate GitHub Release is created correctly
- [ ] Test binary download and installation
- [ ] Document release process in README.md
- [ ] Standardize tag naming convention (use `v*` prefix)
- [ ] Consider adding release templates for consistent release notes

## Notes

- **GitHub Actions runners:** macOS runners are more expensive than Linux (10x cost)
- **Build time:** Rust release builds can take 5-10 minutes
- **Storage:** GitHub provides 500 MB storage for release artifacts (free tier)
- **Tag naming:** Recommend standardizing on `v{major}.{minor}.{patch}` format
- **Branch protection:** Consider requiring tags only from main branch

## References

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Rust Cross-Compilation](https://rust-lang.github.io/rustup/cross-compilation.html)
- [Semantic Versioning](https://semver.org/)
- [GitHub Releases Best Practices](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)
