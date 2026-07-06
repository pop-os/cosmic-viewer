name := "cosmic-viewer"
appid := "com.system76.CosmicViewer"
prefix := "/usr"
bindir := prefix / "bin"
datadir := prefix / "share"

build:
    cargo build

build-release:
    cargo build --release

run *ARGS:
    cargo run -- {{ ARGS }}

run-release *ARGS:
    cargo run --release -- {{ ARGS }}

test:
    cargo test --workspace

bench:
    cargo bench -p viewer-core

clippy:
    cargo clippy --workspace

check:
    cargo check --workspace

clean:
    cargo clean

install: build-release
    sudo install -Dm755 target/release/{{ name }} {{ bindir }}/{{ name }}
    sudo install -Dm644 data/{{ appid }}.desktop {{ datadir }}/applications/{{ appid }}.desktop
    sudo install -Dm644 data/{{ appid }}.metainfo.xml {{ datadir }}/metainfo/{{ appid }}.metainfo.xml
    sudo install -Dm644 data/icons/{{ appid }}-16.svg {{ datadir }}/icons/hicolor/16x16/apps/{{ appid }}.svg
    sudo install -Dm644 data/icons/{{ appid }}-24.svg {{ datadir }}/icons/hicolor/24x24/apps/{{ appid }}.svg
    sudo install -Dm644 data/icons/{{ appid }}-32.svg {{ datadir }}/icons/hicolor/32x32/apps/{{ appid }}.svg
    sudo install -Dm644 data/icons/{{ appid }}-48.svg {{ datadir }}/icons/hicolor/48x48/apps/{{ appid }}.svg
    sudo install -Dm644 data/icons/{{ appid }}-64.svg {{ datadir }}/icons/hicolor/64x64/apps/{{ appid }}.svg
    sudo install -Dm644 data/icons/{{ appid }}-128.svg {{ datadir }}/icons/hicolor/128x128/apps/{{ appid }}.svg
    sudo install -Dm644 data/icons/{{ appid }}-256.svg {{ datadir }}/icons/hicolor/256x256/apps/{{ appid }}.svg
    sudo install -Dm644 data/icons/{{ appid }}-256.svg {{ datadir }}/icons/hicolor/scalable/apps/{{ appid }}.svg

uninstall:
    sudo rm -f {{ bindir }}/{{ name }}
    sudo rm -f {{ datadir }}/applications/{{ appid }}.desktop
    sudo rm -f {{ datadir }}/metainfo/{{ appid }}.metainfo.xml
    sudo rm -f {{ datadir }}/icons/hicolor/{16x16,24x24,32x32,48x48,64x64,128x128,256x256,scalable}/apps/{{ appid }}.svg
