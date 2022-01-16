package main

import (
  "os"
  "errors"
  "log"
  "os/exec"
)

func exists(path string) bool {
  _, err := os.Stat(path)
  return !errors.Is(err, os.ErrNotExist)
}

func execute(command string, dir string) (error) {
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
