#!/bin/bash
rm build -rf
mkdir -p build/linux
mkdir -p build/windows
mkdir -p build/macos
go get
GOOS=linux GOARCH=amd64 go build -o build/linux/spanr-test
GOOS=windows GOARCH=386 go build -o build/windows/spanr-test.exe
GOOS=windows GOARCH=amd64 go build  -o build/windows/spanr-test64.exe
GOOS=darwin GOARCH=amd64 go build  -o build/macos/spanr-test

cp README.MD build/linux
cp README.MD build/windows
cp README.MD build/macos