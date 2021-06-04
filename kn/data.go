package main

import (
	"bufio"
	"errors"
	"fmt"
	"gopkg.in/yaml.v3"
	"io"
	"io/ioutil"
	"os"
	"os/exec"
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
	ReadyState
	GreenState
	DoneState
	UnknownState
)

const (
	ZettleNote NoteType = iota
	MapNote
	LiteratureNote
	FleetingNote
	ReportNote
	UnknownNote
)

const (
	LinkUrl LinkType = iota
	LinkAttachment
	LinkNote
	LinkEmpty
	LinkReport
)

type NoteHeader struct {
	Title    string
	Id       string
	Filename string
	Date     string
	Type     NoteType
	State    NoteState
	Tags     []string
}

type NoteHeaderYaml struct {
	Title string   `yaml:"Title"`
	Date  string   `yaml:"Date"`
	Type  string   `yaml:"Type"`
	State string   `yaml:"State"`
	Tags  []string `yaml:"Tags"`
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
	defer file.Close()

	//Stripping the top ---
	s.Scan()

	if s.Text() != "---" {
		return result, errors.New("File header doesn't start with ---")
	}

	yamlText := ""

	for s.Scan() {
		txt := s.Text()

		if txt == "---" {
			break
		}

		yamlText += txt + "\n"
	}

	var data NoteHeaderYaml
	err = yaml.Unmarshal([]byte(yamlText), &data)
	if err != nil {
		return result, err
	}

	result.Title = data.Title
	result.Date = data.Date
	result.Tags = data.Tags

	if data.Type == "zettle" {
		result.Type = ZettleNote
	} else if data.Type == "literature" {
		result.Type = LiteratureNote
	} else if data.Type == "fleeting" {
		result.Type = FleetingNote
	} else if data.Type == "map" {
		result.Type = MapNote
	} else {
		result.Type = UnknownNote
	}

	if data.State == "new" {
		result.State = NewState
	} else if data.State == "done" {
		result.State = DoneState
	} else if data.State == "ready" {
		result.State = ReadyState
	} else if data.State == "green" {
		result.State = GreenState
	} else {
		result.State = UnknownState
	}

	return result, nil
}

func FindByTag(tag string, noteTypes []NoteType) []NoteHeader {
	result := make([]NoteHeader, 0)

	filtered := FindNotes("", noteTypes)


	for _, note := range filtered {
		for _, t := range note.Tags {
			if t == tag {
				result = append(result, note)
				break
			}
		}
	}

	return result
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

func ExtractLinks(note *NoteData) {
	link := regexp.MustCompile(`\[(.+?)\]\((.+?)\)`)
	linkMatches := link.FindAllStringSubmatch(note.RawText, -1)

	id := 0
	for i := range linkMatches {
		typ := LinkUrl

		if strings.HasPrefix(linkMatches[i][2], "zk:") {
			typ = LinkNote
		}

		if strings.HasPrefix(linkMatches[i][2], "zka:") {
			typ = LinkAttachment
		}

		if strings.HasPrefix(linkMatches[i][2], "rp:") {
			typ = LinkReport
		}

		lnk := NoteLink{Title: linkMatches[i][1], Path: linkMatches[i][2], Type: typ, Id: id}

		note.Links = append(note.Links, lnk)
		id += 1
	}

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
	ExtractLinks(&result)

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

	dstFile, err := os.Create(filepath.Join(NoteDirectory, ".attachments", atomicId+ext))

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

func RemoveNote(id string) {
	os.Remove(filepath.Join(NoteDirectory, id+".md"))

	idx := -1
	for i := range AllNotes {
		if AllNotes[i].Id == id {
			idx = i
			break
		}
	}

	if idx == -1 {
		return
	}

	if idx == 0 {
		AllNotes = AllNotes[1:]
		return
	}

	if idx == len(AllNotes)-1 {
		AllNotes = AllNotes[0 : len(AllNotes)-1]
		return
	}

	AllNotes = append(AllNotes[:idx], AllNotes[idx+1:]...)

}

func RunCommand(exe string, args []string) int {
	cmd := exec.Command(exe, args...)
	cmd.Stderr = os.Stderr
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Run()

	return cmd.ProcessState.ExitCode()
}

func DoDataSync() bool {
	pwd, _ := os.Getwd()
	os.Chdir(NoteDirectory)
	defer os.Chdir(pwd)

	if RunCommand("git", []string{"add", "."}) != 0 {
		return false
	}

	if RunCommand("git", []string{"commit", "-m", "Syncing data"}) != 0 {
		return false
	}

	if RunCommand("git", []string{"push"}) != 0 {
		return false
	}

	return true
}
