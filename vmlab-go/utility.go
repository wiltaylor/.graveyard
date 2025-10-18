package main

import (
	"errors"
	"io"
	"net"
	"os"
	"os/exec"
	"regexp"
)

var outBuff []byte = make([]byte, 1024*1024*100)

func exists(path string) bool {
	_, err := os.Stat(path)
	return !errors.Is(err, os.ErrNotExist)
}

func execute(command string, dir string) error {
	shell := os.Getenv("SHELL")

	cmd := exec.Command(shell, "-c", command)
	if dir != "" {
		cmd.Dir = dir
	}
	cmd.Stderr = os.Stderr
	cmd.Stdout = os.Stdout
	cmd.Stdin = os.Stdin

	err := cmd.Run()

	if err != nil {
		return err
	}

	return nil
}

func readSocket(sock net.Conn) (string, error) {

	//buffer := make([]byte, 1024 * 1024)

	n, err := sock.Read(outBuff[:])

	if err != nil {
		return "", err
	}

	return string(outBuff[0:n]), nil
}

func writeSocket(sock net.Conn, text string) error {
	_, err := io.WriteString(sock, text)
	return err
}

func regexFirstGroup(pattern string, text string) string {
	re := regexp.MustCompile(pattern)

	for _, match := range re.FindAllStringSubmatch(text, -1) {
		for i, group := range match {
			if i == 0 {
				continue
			}
			return group
		}
	}

	return ""
}
