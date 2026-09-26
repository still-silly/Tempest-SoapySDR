# Rust/Slint prototype

This workspace is the native, cross-platform replacement path for the legacy
Swing UI. It currently validates three boundaries:

1. Rust owns and safely frees a `libTSDRLibrary` instance.
2. The Dell 2407WFP EDID timing is represented as data rather than UI constants.
3. Slint can display a continuously replaceable native pixel buffer.

The current image is a generated grayscale test frame. RTL-SDR streaming and
Android packaging are the next milestones.

## Linux development build

From the repository root:

```sh
make -C TempestSDR clean all
LD_LIBRARY_PATH="$PWD/TempestSDR/bin/LINUX/X64" cargo test --manifest-path rust/Cargo.toml
cargo run --manifest-path rust/Cargo.toml -p tempest-ui
```

Set `TSDR_LIBRARY_DIR` when using a library outside the repository's normal
`TempestSDR/bin/LINUX/X64` output directory.
