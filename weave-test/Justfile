build-linux-x86_64:
    cargo build --target x86_64-unknown-linux-musl --release

build-linux-x86:
    cargo build --target i686-unknown-linux-musl --release

build-windows-x86_64:
    cargo build --target x86_64-pc-windows-gnu --release

build-windows-x86:
    cargo build --target i686-pc-windows-gnu --release

build: build-linux-x86_64 build-linux-x86 build-windows-x86_64 build-windows-x86

clean:
    rm -fr ./target
    rm -fr ./release

release: build
    mkdir -p ./release
    cp ./target/x86_64-unknown-linux-musl/release/weave-test ./release/weave-test-x86_64-linux
    cp ./target/i686-unknown-linux-musl/release/weave-test ./release/weave-test-x86-linux
    cp ./target/x86_64-pc-windows-gnu/release/weave-test.exe ./release/weave-test-x86_64-windows.exe
    cp ./target/i686-pc-windows-gnu/release/weave-test.exe ./release/weave-test-x86-windows.exe
