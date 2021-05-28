package main

import (
	"bufio"
	"errors"
	"fmt"
  "io"
	"io/ioutil"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

type NoteState int
type NoteType int
type LinkType int

const (
	NewState NoteState = iota
	GreenState
	DoneState
	UnknownState
)

const (
	ZettleNote NoteType = iota
	MapNote
	LiteratureNote
	FleetingNote
	UnknownNote
)

const (
	LinkUrl LinkType = iota
	LinkAttachment
	LinkNote
	LinkEmpty
)

type NoteHeader struct {
	Title    string
	Id       string
	Filename string
	Date     string
	Type     NoteType
	State    NoteState
}

type NoteLink struct {
	Title string
	Type  LinkType
	Path  string
	Id    int
}

type NoteData struct {
	Header       NoteHeader
	RawText      string
	FormatedText string
	Links        []NoteLink
}

var AllNotes []NoteHeader

func GetHeaderFromFile(id string) (NoteHeader, error) {
	path := filepath.Join(NoteDirectory, id+".md")
	result := NoteHeader{Title: "", Id: id, Filename: filepath.Join(NoteDirectory, id+".md"), Date: "", Type: UnknownNote, State: UnknownState}

	file, err := os.Open(path)

	if err != nil {
		return result, err
	}

	s := bufio.NewScanner(file)

	//Stripping the top ---
	s.Scan()

	if s.Text() != "---" {
		return result, errors.New("File header doesn't start with ---")
	}

	for s.Scan() {
		txt := s.Text()

		if txt == "---" {
			break
		}

		if strings.HasPrefix(txt, "Title:") {
			result.Title = strings.Trim(txt[6:], " ")
			continue
		}

		if strings.HasPrefix(txt, "Date:") {
			result.Date = strings.Trim(txt[5:], " ")
			continue
		}

		if strings.HasPrefix(txt, "Type:") {
			typ := strings.Trim(txt[5:], " ")

			if typ == "zettle" {
				result.Type = ZettleNote
				continue
			}

			if typ == "literature" {
				result.Type = LiteratureNote
				continue
			}

			if typ == "fleeting" {
				result.Type = FleetingNote
				continue
			}

			if typ == "map" {
				result.Type = MapNote
				continue
			}

			result.Type = UnknownNote
			continue
		}

		if strings.HasPrefix(txt, "Status:") {
			status := strings.Trim(txt[7:], " ")

			if status == "new" {
				result.State = NewState
				continue
			}

			if status == "done" {
				result.State = DoneState
				continue
			}

			if status == "green" {
				result.State = GreenState
				continue
			}

			result.State = UnknownState
			continue
		}
	}

	return result, nil
}

func FindNotes(pattern string, noteTypes []NoteType) []NoteHeader {
	result := make([]NoteHeader, 0)

  pattern = strings.ToLower(pattern)

	for _, note := range AllNotes {
		m := false
		for _, t := range noteTypes {
			if note.Type == t {
				m = true
				break
			}
		}

		if m == false {
			continue
		}

		match, _ := regexp.MatchString(pattern, strings.ToLower(note.Title))

		if match {
			result = append(result, note)
		}
	}

	return result
}

func RefreshNotes() error {
	AllNotes = make([]NoteHeader, 0)

	files, err := ioutil.ReadDir(NoteDirectory)

	if err != nil {
		return err
	}

	for _, file := range files {
		if file.IsDir() {
			continue
		}

		matched, err := regexp.MatchString(`\.md$`, file.Name())
		if err != nil {
			continue
		}

		if !matched {
			continue
		}

		id := strings.ReplaceAll(file.Name(), ".md", "")
		header, err := GetHeaderFromFile(id)

		if err != nil {
			continue
		}

		AllNotes = append(AllNotes, header)

	}

	return nil
}

func RefreshNote(id string) error {
  idx := -1

	for i, note := range AllNotes {
		if note.Id == id {
			idx = i
			break
		}
	}

	if idx == -1 {
		return errors.New("No note with that name found!")
	}

	newNote, err := GetHeaderFromFile(id)
	AllNotes[idx] = newNote

	return err
}

func NewNote(title string, noteType NoteType) (NoteData, error) {
	curTime := time.Now().UTC()
	atomicId := fmt.Sprintf("%v", curTime.Unix())
	path := filepath.Join(NoteDirectory, fmt.Sprintf("%v.md", atomicId))

	header := NoteHeader{Title: title, Id: atomicId, Filename: path, Date: curTime.Format(time.RFC822)}
	result := NoteData{Header: header, RawText: "", FormatedText: "", Links: make([]NoteLink, 0)}

	err := SaveNoteData(result)

  AllNotes = append(AllNotes, header)

	return result, err
}

func GetNoteData(header NoteHeader) (NoteData, error) {
	result := NoteData{Header: header, RawText: "", FormatedText: "", Links: make([]NoteLink, 0)}

	byteData, err := ioutil.ReadFile(header.Filename)

	if err != nil {
		return result, err
	}

	text := string(byteData)

	index := 0
	remain := 2
	currentCount := 0

	for i, c := range text {
		if c == '-' {
			currentCount += 1
		} else {
			currentCount = 0
		}

		if currentCount == 3 {
			currentCount = 0
			remain -= 1
		}

		if remain == 0 {
			index = i + 2 // Also eat new line
			break
		}
	}

	text = text[index:]

	result.RawText = text

	link := regexp.MustCompile(`\[(.+)\]\((.+)\)`)
	linkMatches := link.FindAllStringSubmatch(text, -1)

	id := 0
	for i := range linkMatches {
		typ := LinkUrl

		if strings.HasPrefix(linkMatches[i][2], "zk:") {
			typ = LinkNote
		}

		if strings.HasPrefix(linkMatches[i][2], "zka:") {
			typ = LinkAttachment
		}

		lnk := NoteLink{Title: linkMatches[i][1], Path: linkMatches[i][2], Type: typ, Id: id}

		result.Links = append(result.Links, lnk)
		id += 1
	}

	return result, nil
}

func SaveNoteData(note NoteData) error {
	noteTypeText := "zettle"
	statusText := "new"

	if note.Header.Type == ZettleNote {
		noteTypeText = "zettle"
	}

	if note.Header.Type == MapNote {
		noteTypeText = "map"
	}

	if note.Header.Type == LiteratureNote {
		noteTypeText = "literature"
	}

	if note.Header.Type == FleetingNote {
		noteTypeText = "fleeting"
	}

	if note.Header.State == NewState {
		statusText = "new"
	}

	if note.Header.State == DoneState {
		statusText = "done"
	}

	if note.Header.State == GreenState {
		statusText = "green"
	}

	file, err := os.Create(note.Header.Filename)

	if err != nil {
		return err
	}

	writer := bufio.NewWriter(file)
	writer.WriteString("---\n")
	writer.WriteString(fmt.Sprintf("Title: %s\n", note.Header.Title))
	writer.WriteString(fmt.Sprintf("Date: %s\n", note.Header.Date))
	writer.WriteString(fmt.Sprintf("Type: %s\n", noteTypeText))
	writer.WriteString(fmt.Sprintf("Status: %s\n", statusText))
	writer.WriteString("---\n")
	writer.WriteString(note.RawText)
	writer.Flush()
	file.Close()

	return nil

}

func AttachFile(path string) string {
  ext := filepath.Ext(path)
	curTime := time.Now().UTC()
	atomicId := fmt.Sprintf("%v", curTime.Unix())

  dstFile, err := os.Create(filepath.Join(NoteDirectory, ".attachments", atomicId + ext))

  if err != nil {
    panic(err)
  }

  defer dstFile.Close()

  srcFile, err := os.Open(path)

  if err != nil {
    panic(err)
  }

  defer srcFile.Close()

  io.Copy(dstFile, srcFile)

  return atomicId
}
