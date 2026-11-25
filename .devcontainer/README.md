# Mininet Rust development container

This VS Code Dev Container keeps the Rust compiler, Python 3, Linux build tools, MinGW cross-compiler, and Cargo build outputs in Docker. It follows the repository's `rust-toolchain.toml` pin (`1.95.0`) and installs the `wasm32-wasip2` and `x86_64-pc-windows-gnu` targets used by CI. Python 3 runs the native Forge acceptance script.

Install the Microsoft Dev Containers extension in VS Code, open the Mininet repository, and run **Dev Containers: Reopen in Container**. VS Code installs Rust Analyzer and TOML support in the container. The named Cargo and build-cache volumes survive container rebuilds, so dependencies do not need to be installed on the Windows host and builds do not use the host's `target` directory. Build outputs use a subdirectory named for the workspace folder, keeping separate worktrees from sharing Cargo artifacts.

The container is a Linux development and test environment. Its Windows GNU target reproduces CI's Windows-conditional Clippy/type-check; it does not run Windows binaries or validate Windows shell behavior. It does not validate Android builds, device behavior, real-target installation, or release approval.

Run the same core checks used by Linux CI from the container terminal:

```sh
cargo fmt --all -- --check
cargo clippy --keep-going --all-targets --all-features -- -D warnings
cargo test --all --all-features
python3 tools/check_native_forge.py --release
cargo clippy --target x86_64-pc-windows-gnu --all-targets --all-features \
  -p mini-windows-vault -p mini-windows-setup -p mini-setup \
  -p mini-app-protocol -p mini-app-service -p mini-selftest -p mini-desktop \
  -- -D warnings
```
