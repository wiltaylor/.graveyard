package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
)

var NoteDirectory string

func main() {

	attach := flag.String("a", "", "Copies file into attachment folder and returns the id")
	flag.Parse()

	NoteDirectory = os.Getenv("ZKDIR")

	os.MkdirAll(filepath.Join(NoteDirectory, ".attachments"), 0760)

	if *attach != "" {
		id := AttachFile(*attach)
		fmt.Println(id)
		return
	}

	RefreshNotes()

	InitUI()
	RunUI()
}
