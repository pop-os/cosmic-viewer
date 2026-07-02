name := "cosmic-viewer"
appid := "com.system76.CosmicViewer"
prefix := "/usr"
bindir := prefix / "bin"
datadir := prefix / "share"

build:
    cargo build

release:
    cargo build --release

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

install: release
    install -Dm755 target/release/{{name}} {{bindir}}/{{name}}
    install -Dm644 data/{{appid}}.desktop {{datadir}}/applications/{{appid}}.desktop
    install -Dm644 data/icons/{{appid}}-256.svg {{datadir}}/icons/hicolor/apps/scalable/{{appid}}.svg
    install -Dm644 data/icons/{{appid}}-256.svg {{datadir}}/icons/hicolor/apps/256x256/{{appid}}.svg

uninstall:
    rm -f {{bindir}}/{{name}}
    rm -f {{datadir}}/applications/{{appid}}.desktop
    rm -f {{datadir}}/icons/hicolor/apps/scalable/{{appid}}.svg
    rm -f {{datadir}}/icons/hicolor/apps/256x256/{{appid}}.svg
