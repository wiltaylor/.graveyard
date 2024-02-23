# Weave Test
Weave Test is a simple testing tool originally designed to work with Config-Weave but could also be used standalone.

## Features
- Portable - Can just drop the binary on a system and run the tests.
- Cross Platform - Runs on Linux, Windows and macOS.
- Simple interface - Works with environment variables and stdout. Can write tests in shell scripts or any other language.

## How to install
```shell
cargo install https://github.com/wiltaylor/test-weave
```

Or just grab an executable form the releases tab and make it executable.


## Getting Started
To create a 

## Design Principles
### Language Agnostic Test Interface
Weave Test doesn't specify which language you write your tests in. All it cares about is you can read environment variables
to get the configuration and you can write specific strings to stdout to tell Weave Test if the test passed or not.

While this might seem a little simplistic and requires a little bit of boilerplate in your scripts it gives the 
test writer a lot of options when writing tests.

### Static Binaries
Weave Test is designed to be compiled as a static binary and copied to a target system to run tests with.

This makes it easy to use Weave Test to QA an environment after its built or test something in a restricted environment
like a trimmed down container.

Also means tests can be removed from a system without leaving anything behind.

### Small Core
Weave Test is designed to have as few features in the core as possible. If it is possible to implement a feature
in the language a script is being written in then that is the ideal location to do it.

I am trying to follow the unix philosophy of doing one thing well.

### Cross Platform
This project should be buildable for Linux, Windows and macOS.

