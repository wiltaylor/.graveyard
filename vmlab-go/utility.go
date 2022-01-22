package main

import (
  "errors"
	"os"
	"os/exec"
  "net"
  "io"
)


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

func readSocket(sock net.Conn)(string, error) {

  buffer := make([]byte, 1024 * 1024)

  n, err := sock.Read(buffer[:])

  if err != nil {
    return "", err
  }

  return string(buffer[0:n]), nil
}

func writeSocket(sock net.Conn, text string) error {
  _, err := io.WriteString(sock, text)
  return err
}
