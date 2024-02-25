# Weave Test
Weave Test is a simple testing tool originally designed to work with Config-Weave but could also be used standalone.

## Features
- Portable - Can just drop the binary on a system and run the tests.
- Cross Platform - Runs on Linux and Windows.
- Simple interface - Works with environment variables and stdout. Can write tests in shell scripts or any other language.

## How to install
```shell
cargo install https://github.com/wiltaylor/test-weave
```

Or just grab an executable form the releases tab and make it executable.

## Getting Started
Quickest way to get started is to have a look at the example folder of this repository.

weave-test will run any file in the target folder that ends with test.yaml.

## Building
To build the project you first need to have rust installed and then install the following toolchains:

- make sure mingw is installed from your distros package manager.

```shell
rustup target add x86_64-pc-windows-gnu
rustup target add x86_64-unknown-linux-musl
rustup target add i686-pc-windows-gnu
rustup target add i686-unknown-linux-musl
```