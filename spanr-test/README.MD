# SPANR Testing Tool
This is a simple tool designed to assist with testing SPANR resources 
but there is no reason it couldn't be used for other things.

The goal of this tool is to follow the UNIX philosphy for doing one
thing and doing it well.

So this tool can execute tests writen in any language that can write 
to stdout and will generate a report at the end on tests that have
run and which ones have failed.

## Installation
Installation is really simple you just need to grab the executable from
release and run it where it is or copy it into /usr/bin. The
portability of the binary is due to how its used with SPANR. It is 
expected to be copied to a system under configuration with SPANR and
then removed once everything is installed and finished.

There are binaries for Linux, Windows and macOS.

## Building
To build simply run the following:

```bash
$> go get
$> go build
```

