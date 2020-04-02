# BLDR - Simple build orchestration tool
BLDR is a build tool that uses containers to isolate build tool chains. This
allows for the same tool chain to be used on developer machines and Continuous
integration servers.

## Building
There are 2 ways to build the project.

BLDR way:
```bash
$ bldr build
$ bldr package
```

Manual way (if you don't have bldr installed):
```bash
$ cargo build --release
```

Once you have the above built the executable will be located in target/release
as a single binary (bldr or bldr.exe depending on the platform).

## Installing
Simply put bldr executable in your path.

## Using
To use bldr you first must createa  bldr folder in your project and a bldr.yaml
file in the root of your project.

### blrd folder
In the bldr folder you need to create docker files for the tool chains you want
to create. Create them with the naming format of **Dockerfile-toolchainname**
