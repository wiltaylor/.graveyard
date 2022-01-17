package main

import (
	"errors"
	"log"
	"os"
	"os/exec"
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
		log.Fatal(err)
		return err
	}

	return nil
}

func getCommandAndArgs(args []string) (string, []string, error) {
	if len(args) == 0 {
		return "", make([]string, 0), errors.New("No arguments passed in")
	}

	command := args[0]
	commandArgs := make([]string, 0)

	if len(args) > 1 {
		commandArgs = args[1:]
	}

	return command, commandArgs, nil
}
