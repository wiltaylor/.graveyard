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

Now you can run:
```shell
just release
```

This will build all versions of the software and put them in a release folder ready for copying to a machine.

## Contributing 
If you want to add to the project I recommend raising an issue to discuss the addition before doing a pr.

I am open to bug fixes but new features will be added sparingly. I am trying to keep the core of this project small.

## License: MIT
Copyright 2024 Wil Taylor

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the “Software”), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.



