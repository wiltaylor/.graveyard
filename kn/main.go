package main

import (
  "os"
)

var NoteDirectory string

func main() {

  NoteDirectory = os.Getenv("ZKDIR")

  RefreshNotes()

  InitUI()
  RunUI()
}
