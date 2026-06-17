name := "cosmic-viewer"
appid := "com.system76.CosmicViewer"
prefix := "/usr/local"
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

uninstall:
    rm -f {{bindir}}/{{name}}
    rm -f {{datadir}}/applications/{{appid}}.desktop
