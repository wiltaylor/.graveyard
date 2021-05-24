package main

import (
	"bufio"
	"flag"
	"fmt"
	"io/ioutil"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/marcusolsson/tui-go"
)

type NoteType int
const (
  Zettle NoteType = iota
  Map
  Literature
  Fleeting
  UnknownType
)

type NoteState int
const (
  New NoteState = iota
  EverGreen
  Done
  UnknownState
)

type NoteRecord struct {
  Title string
  Date string
  Type NoteType
  State NoteState
  }

  func GetNoteMeta() []NoteRecord {
    result := make([]NoteRecord, 0)

    files, err := ioutil.ReadDir(getZKPath())

    if err != nil {
      panic(err)
    }

    for _, file := range files {
      if file.IsDir() {
        continue
      }

      matched, err := regexp.MatchString(`\.md$`, file.Name())
      if err != nil {
        panic(err)
      }

      if !matched {
        continue
      }

      fileHandle, err := os.Open(filepath.Join(getZKPath(), file.Name()))
      if err != nil {
        panic(err)
      }

      s := bufio.NewScanner(fileHandle)
      s.Scan() // Get rid of first line
      if s.Text() != "---" {
        panic(fmt.Sprintf("Expected first line of %s to be ---", file.Name()))
      }

      curRec := NoteRecord{Title: "", Date: "", Type: UnknownType, State: UnknownState}

      for s.Scan() {

        txt := s.Text()

        if txt == "---" {
          break
        }

        if strings.HasPrefix(txt, "Title:") {
          curRec.Title = strings.Trim(txt[6:], " ")
          continue
        }

        if strings.HasPrefix(txt, "Date:") {
          curRec.Date = strings.Trim(txt[5:], " ")
          continue
        }

        if strings.HasPrefix(txt, "Type:") {
          noteType := strings.Trim(txt[5:], " ")

          if noteType == "zettle" {
            curRec.Type = Zettle
            continue
          }

          if noteType == "literature" {
            curRec.Type = Literature
            continue
          }

          if noteType == "fleeting" {
            curRec.Type = Fleeting
            continue
          }

          if noteType == "map" {
            curRec.Type = Map
            continue
          }

          curRec.Type = UnknownType
          continue
        }

        if strings.HasPrefix(txt, "Status:") {
          status := strings.Trim(txt[7:], " ")

          if status == "new" {
            curRec.State = New
            continue
          }

          if status == "done" {
            curRec.State = Done
            continue
          }

          if status == "green" {
            curRec.State = EverGreen
            continue
          }

          curRec.State = UnknownState
          continue
        }
      }

      result = append(result, curRec)
    }


    return result
  }

func usage() {
	fmt.Println("kn command [options]")
	fmt.Println("")
	fmt.Println("Commands:")
	fmt.Println("new {l,z,f,m}- Create a new note.")
	fmt.Println("   l - literature note")
	fmt.Println("   z - zettle note")
	fmt.Println("   f - fleeting note")
	fmt.Println("   m - map of content note")

}

func getZKPath() string {
	notePath := os.Getenv("ZKDIR")
	if notePath == "" {
		notePath = "./.zk"
	}

  return notePath
}

func newNote(note_type string) {

  notePath := getZKPath()

	switch note_type {
	case "l", "literature":
		note_type = "literature"
	case "z", "zettle":
		note_type = "zettle"
	case "f", "fleeting":
		note_type = "fleeting"
	case "m", "map":
		note_type = "map"
	default:
		panic("Unexpected note type " + note_type + "!")
	}

	curTime := time.Now().UTC()
	atomicId := fmt.Sprintf("%v", curTime.Unix())

	filePath := fmt.Sprintf("%s/%s.md", notePath, atomicId)

	file, err := os.Create(filePath)

	if err != nil {
		panic("Can't create note!")
	}

	writer := bufio.NewWriter(file)
	writer.WriteString("---\n")
	writer.WriteString("Title: New Note\n")
	writer.WriteString("Date: " + curTime.Format(time.RFC822) + "\n")
	writer.WriteString("Type: " + note_type + "\n")
	writer.WriteString("Status: new\n")
	writer.WriteString("---\n")

	writer.Flush()
	file.Close()

	fmt.Println(filePath)

	cmd := exec.Command("vim", filePath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Stdin = os.Stdin

	err = cmd.Run()

	if err != nil {
		panic(err)
	}

	fmt.Println(atomicId)
}

func searchInNotes(text string, notes *[]NoteRecord, typ NoteType) []string { 
  result := make([]string, 0)

  for _, n := range *notes {
    if n.Type == typ {
      match, err := regexp.MatchString(text, n.Title)

      if err != nil {
       panic(err)
      }

      if match {
        result = append(result, n.Title)
      }
    }
  }

  return result
}

func search(text string) {

  allNotes := GetNoteMeta()
  selectedNoteType := Zettle

  searchBox := tui.NewEntry()
  searchBox.SetFocused(true)
  searchBox.SetSizePolicy(tui.Expanding, tui.Maximum)
  searchBox.SetText(text)


  searchResult := tui.NewList()
  searchResult.SetFocused(true)

  for _, note := range searchInNotes(text, &allNotes, selectedNoteType) {
    searchResult.AddItems(note)
  }

  searchBlock := tui.NewVBox(
    tui.NewHBox(
      tui.NewLabel("Search: "),
      searchBox,
    ),
    tui.NewLabel("Results: "),
    tui.NewLabel("---------"),
    searchResult,
    tui.NewSpacer(),
  )
  searchBlock.SetBorder(true)
  searchBlock.SetSizePolicy(tui.Expanding, tui.Expanding)


  noteType := tui.NewList()
  noteType.AddItems("Zettle", "Map", "Literature", "Fleeting", "Unknown")
  noteType.SetSelected(0)
  noteType.SetSizePolicy(tui.Minimum, tui.Minimum)


  noteTypeBox := tui.NewVBox(
    tui.NewLabel("Search For Note Type:"),
    noteType,
  )

  noteTypeBox.SetBorder(true)
  noteTypeBox.SetSizePolicy(tui.Minimum, tui.Minimum)

  box := tui.NewVBox(
    searchBlock,
    noteTypeBox,
  )

  ui, err := tui.New(box)
  if err != nil {
    panic(err)
  }

  searchBox.OnChanged(func(entry *tui.Entry) {
    searchResult.RemoveItems()
    results := searchInNotes(entry.Text(), &allNotes, selectedNoteType)

    for _, r := range results {
      searchResult.AddItems(r)
    }
  })

  noteType.OnSelectionChanged(func (lst *tui.List) {
    if lst.SelectedItem() == "Zettle" {
      selectedNoteType = Zettle
    }

    if lst.SelectedItem() == "Map" {
      selectedNoteType = Map
    }

    if lst.SelectedItem() == "Literature" {
      selectedNoteType = Literature
    }

    if lst.SelectedItem() == "Fleeting" {
      selectedNoteType = Fleeting
    }

    if lst.SelectedItem() == "Unknown" {
      selectedNoteType = UnknownType
    }

    searchResult.RemoveItems()
    results := searchInNotes(searchBox.Text(), &allNotes, selectedNoteType)

    for _, r := range results {
      searchResult.AddItems(r)
    }
  })


  ui.SetKeybinding("Esc", func() { ui.Quit() })
  ui.SetKeybinding("Tab", func() {
    if searchBox.IsFocused() {
      noteType.SetFocused(true)
      searchBox.SetFocused(false)
      searchResult.SetFocused(false)
      return
    }

    if noteType.IsFocused() {
      searchBox.SetFocused(true)
      searchResult.SetFocused(true)
      noteType.SetFocused(false)
      return
    }
  })


  if err := ui.Run(); err != nil {
    panic(err)
  }
}

func main() {
	flag.Parse()
	cmd := flag.Arg(0)

	if flag.NArg() < 2 {
		usage()
		return
	}

	switch cmd {
  case "search":
    search("foo")
	case "new":
		newNote(flag.Arg(1))
	default:
		usage()
	}
}
