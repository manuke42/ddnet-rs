Cloning
-------

To clone this repository use:

    git clone --recursive https://github.com/ddnet/ddnet-rs

To clone the submodules if you have previously cloned DDNet-rs without them, or if you require the full history instead of a shallow clone:

    git submodule update --init --recursive

Building
--------

In order to build DDNet-rs you need the latest stable rust compiler and a c compiler:

- Visit https://rustup.rs/ to install rust, make sure rust is up to date `rustup update`
- Inside the project directory open a terminal and type `cargo run --release`

Features
--------

Some features require you to compile DDNet-rs with explicit features:
```
cargo run --release --features bundled_data_dir,ffmpeg,microphone,enable_steam
```

- `ffmpeg` enables the demo to video recorder on supported platforms. [Linux]
- `bundled_data_dir` bundles the whole data directory into the executable, making it very portable, but much bigger.
- `microphone` enables the microphone backend which allows features like spatial chat.
- `enable_steam` enables steam support, the resulting binary has to be executed inside a steam runtime to work.

Terminal Client
---------------

A standalone terminal client for deterministic, tick-controlled gameplay:

```bash
# Build the terminal client
cargo build --release -p terminal-client

# Run
./target/release/terminal-client --server 127.0.0.1:8303
```

See `src/terminal-client/README.md` and `DETERMINISTIC_CONTROL.md` for details on:
- Deterministic tick control
- Input via Unix sockets
- Frame output
- ML training and automated testing

Android
-------

```
# using https://github.com/rust-mobile/xbuild
x build --release --arch arm64 --platform android --format apk -p ddnet-rs --features bundled_data_dir
```
