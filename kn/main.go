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

	//"github.com/marcusolsson/tui-go"
	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
)

const (
  LinkIconUrl = ""
  LinkIconNote = ""
  LinkIconAttachment = ""
)

type LinkType int

const (
  LinkUrl LinkType =  iota
  LinkAttachment
  LinkNote
)

type NoteType int

const (
	Zettle NoteType = iota
	Map
	Literature
	Fleeting
	UnknownType
)

type viewExitResult int

const (
  ViewExitOk viewExitResult = iota
  ViewExitFind
  ViewExitReopen
  ViewExitNew
  ViewExitBack
  ViewExitOpen
)

type NoteState int

const (
	New NoteState = iota
	EverGreen
	Done
	UnknownState
)

type LinkRecord struct {
  Title string
  Type LinkType
  Path string
  Location int
  Id string
}

type NoteRecord struct {
  Filename string
	Id       string
  Title    string
  Date     string
  Type     NoteType
	State    NoteState
}

type NoteData struct {
  Header NoteRecord
  Text string
  Links []LinkRecord
}

func GetNoteRecordFromFile(id string) NoteRecord {

  filePath := filepath.Join(getZKPath(), id + ".md")
  result := NoteRecord{Title: "", Date: "", Type: UnknownType, State: UnknownState, Filename:filePath, Id: id}
  file, err := os.Open(filePath)

  if err != nil {
    panic(err)
  }

  s := bufio.NewScanner(file)

  //Strip top --- and fail if not there
  s.Scan()
  if s.Text() != "---" {
    panic(fmt.Sprintf("Expected first line of %s to be ---", file.Name()))
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
      noteType := strings.Trim(txt[5:], " ")

      if noteType == "zettle" {
        result.Type = Zettle
        continue
      }

      if noteType == "literature" {
        result.Type = Literature
        continue
      }

      if noteType == "fleeting" {
        result.Type = Fleeting
        continue
      }

      if noteType == "map" {
        result.Type = Map
        continue
      }

      result.Type = UnknownType
      continue
    }

    if strings.HasPrefix(txt, "Status:") {
      status := strings.Trim(txt[7:], " ")

      if status == "new" {
        result.State = New
        continue
      }

      if status == "done" {
        result.State = Done
      }

      if status == "green" {
        result.State = EverGreen
      }

      result.State = UnknownState
      continue
    }
  }

  return result
}

//TODO: Need to replace this with a yaml parser to make it nice and neat
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

    id := strings.ReplaceAll(file.Name(), ".md", "")
    curRec := GetNoteRecordFromFile(id)
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

func newNote(note_type string) NoteRecord {

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

  editFile(filePath)

	fmt.Println(atomicId)

  return GetNoteRecordFromFile(string(atomicId))
}

func searchInNotes(text string, notes *[]NoteRecord, typ NoteType) []NoteRecord {
	result := make([]NoteRecord, 0)

	for _, n := range *notes {
		if n.Type == typ {
			match, err := regexp.MatchString(text, n.Title)

			if err != nil {
				panic(err)
			}

			if match {
        var r NoteRecord = n
				result = append(result, r)
			}
		}
	}

	return result
}

func escapeForRegex(text string) string {
  text = strings.ReplaceAll(text, "\\", "\\\\")
  text = strings.ReplaceAll(text, "(", "\\(")
  text = strings.ReplaceAll(text, ")", "\\)")
  text = strings.ReplaceAll(text, "[", "\\[")
  text = strings.ReplaceAll(text, "]", "\\]")
  text = strings.ReplaceAll(text, "?", "\\?")
  text = strings.ReplaceAll(text, ".", "\\.")
  text = strings.ReplaceAll(text, "+", "\\+")
  text = strings.ReplaceAll(text, "*", "\\*")
  text = strings.ReplaceAll(text, "{", "\\{")
  text = strings.ReplaceAll(text, "}", "\\}")
  text = strings.ReplaceAll(text, "$", "\\$")
  text = strings.ReplaceAll(text, "^", "\\^")
  text = strings.ReplaceAll(text, "|", "\\|")
  return text
}

func openNoteData(note NoteRecord) NoteData {
  data, err := ioutil.ReadFile(note.Filename)

  if err != nil {
    panic(err)
  }

  text := string(data)

  index := 0
  remain := 2;
  currentCount := 0

  for i, c := range text {
    if c == '-' {
      currentCount += 1
    }else{
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

  h6 := regexp.MustCompile(`###### (.+)\n`)
  h5 := regexp.MustCompile(`##### (.+)\n`)
  h4 := regexp.MustCompile(`#### (.+)\n`)
  h3 := regexp.MustCompile(`### (.+)\n`)
  h2 := regexp.MustCompile(`## (.+)\n`)
  h1 := regexp.MustCompile(`# (.+)\n`)
  link := regexp.MustCompile(`\[(.+)\]\((.+)\)`)

  linkMatches := link.FindAllStringSubmatch(text, -1)
  links := make([]LinkRecord, 0)

  id := 0
  for i := range linkMatches {
    typ := LinkUrl
    ico := LinkIconUrl

    if strings.HasPrefix(linkMatches[i][2], "zk:") {
      typ = LinkNote
      ico = LinkIconNote
    }

    if strings.HasPrefix(linkMatches[i][2], "zka:") {
      typ = LinkAttachment
      ico = LinkIconAttachment
    }

    lnk := LinkRecord{Title: linkMatches[i][1], Path: linkMatches[i][2], Type: typ, Id: fmt.Sprintf("l%d", id), Location: 0}
    links = append(links, lnk)

    lnkText := fmt.Sprintf("[%s](%s)", lnk.Title, lnk.Path)
    lnk.Location = strings.Index(text, lnkText)
    text = strings.Replace(text, lnkText, fmt.Sprintf("[\"%s\"]%s[blue::u]%s[-:-:-][\"\"]", lnk.Id, ico, lnk.Title), 1)
    id += 1
  }

  b1 := regexp.MustCompile(`\n [-|*|+] `)
  b2 := regexp.MustCompile(`\n   [-|*|+] `)
  b3 := regexp.MustCompile(`\n     [-|*|+] `)
  l1 := regexp.MustCompile(`\n ([0-9]{1,5})\. `)
  l2 := regexp.MustCompile(`\n   ([0-9a-z]{1,5})\. `)
  l3 := regexp.MustCompile(`\n     ([0-9a-z]{1,5})\. `)

  text = h6.ReplaceAllString(text, "     [green::b] $1[-:-:-]\n")
  text = h5.ReplaceAllString(text, "    [green::b] $1[-:-:-]\n")
  text = h4.ReplaceAllString(text, "   [green::b] $1[-:-:-]\n")
  text = h3.ReplaceAllString(text, "  [green::b] $1[-:-:-]\n")
  text = h2.ReplaceAllString(text, " [blue::b] $1[-:-:-]\n")
  text = h1.ReplaceAllString(text, "[red::b] $1[-:-:-]\n")

  text = b1.ReplaceAllString(text, "\n [green]ﱣ[-] $1")
  text = b2.ReplaceAllString(text, "\n   [green]ﱤ[-] $1")
  text = b3.ReplaceAllString(text, "\n     [green][-] $1")
  text = l1.ReplaceAllString(text, "\n [green]$1.[-]")
  text = l2.ReplaceAllString(text, "\n   [green]$1.[-]")
  text = l3.ReplaceAllString(text, "\n     [green]$1.[-]")

  text = link.ReplaceAllString(text, " [blue::u]$1[::-] ")

  return NoteData{Header: note, Text: text, Links: links}
}

func editFile(path string) {
  editor := os.Getenv("EDITOR")

	cmd := exec.Command(editor, path)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Stdin = os.Stdin

  err := cmd.Run()

	if err != nil {
		panic(err)
	}

}

func viewUI(doc NoteData) (viewExitResult, string){
  app := tview.NewApplication()
  textView := tview.NewTextView().SetDynamicColors(true).SetRegions(true).SetWordWrap(true).SetChangedFunc(func() {
    app.Draw()
  })

  textView.SetText(doc.Text)
  textView.SetScrollable(true)
  textView.SetTitle(doc.Header.Title)
  textView.SetBorder(true)
  textView.SetBorderPadding(0,0,1,1)
  app.SetFocus(textView)
  exitCode := ViewExitOk
  selectedLink := -1
  returnId := ""

  toolBar := tview.NewTextView()
  toolBar.SetText("ESC - Quit | N - New | F - Find | E - Edit | HJKL - Move | Enter - Follow Link | Backspace - Previous Note")
  toolBar.SetBackgroundColor(tcell.ColorWhite)
  toolBar.SetTextColor(tcell.ColorBlack)

  layout := tview.NewGrid()
  layout.SetRows(1, 5, 0)
  layout.AddItem(toolBar, 0, 0, 1, 1, 1, 1, false)
  layout.AddItem(textView, 1, 0, 10, 1, 10, 40, true)

	app.SetInputCapture(func(key *tcell.EventKey) *tcell.EventKey {

		if key.Key() == tcell.KeyEscape {
			app.Stop()
			return nil
		}

    if key.Key() == tcell.KeyTab  {

      selectedLink += 1

      if selectedLink >= len(doc.Links) {
        selectedLink = 0
      }

      textView.Highlight(doc.Links[selectedLink].Id)
      textView.ScrollToHighlight()

      return nil

    }

    if key.Key() == tcell.KeyBackspace || key.Key() == tcell.KeyBackspace2 {
      exitCode = ViewExitBack
      app.Stop()
    }

    if key.Key() == tcell.KeyEnter {
      if textView.HasFocus() {
        if selectedLink != -1 {
          if doc.Links[selectedLink].Type == LinkUrl {
            cmd := exec.Command("xdg-open", doc.Links[selectedLink].Path)
            cmd.Run()
            return nil
          }

          if doc.Links[selectedLink].Type == LinkNote {
            returnId = strings.ReplaceAll(doc.Links[selectedLink].Path, "zk:", "")
            exitCode = ViewExitOpen
            app.Stop()
            return nil
          }

          if doc.Links[selectedLink].Type == LinkAttachment {
            attach := strings.ReplaceAll(doc.Links[selectedLink].Path, "zka:", "")
            attachPath := filepath.Join(getZKPath(), "attachments", attach)

            cmd := exec.Command("xdg-open", attachPath)
            cmd.Run()
            return nil
          }
        }
      }
    }

    if key.Rune() == 'e' {
      editFile(doc.Header.Filename)
      exitCode = ViewExitReopen
      app.Stop()
      return nil
    }

    if key.Rune() == 'f' {
      exitCode = ViewExitFind
      app.Stop()
      return nil
    }

    if key.Rune() == 'n' {
      exitCode = ViewExitNew
      app.Stop()
      return nil
    }

    if key.Rune() == 'l' {

      searchUI("")

      return nil
    }

    return key
  })

  if err := app.SetRoot(layout, true).Run(); err != nil {
    panic(err)
  }

  return exitCode, returnId
}

func searchUI(text string) *NoteRecord {

	allNotes := GetNoteMeta()
  var searchNotes []NoteRecord
	selectedNoteType := Zettle
  var selectedNote *NoteRecord
  selectedNote = nil

	app := tview.NewApplication()

	searchResult := tview.NewList()
	searchResult.ShowSecondaryText(false)
	searchField := tview.NewInputField().SetLabel("Search")
	searchField.SetText(text)

	searchNoteType := tview.NewDropDown()

	searchResult.Clear()

	{
		searchNotes = searchInNotes(text, &allNotes, selectedNoteType)
		for _, n := range searchNotes {
			searchResult.AddItem(n.Title, "", '\n', func() {
				selectedNote = &n
				app.Stop()
			})
		}
	}

	searchNoteType.SetOptions([]string{"Zettle", "Litrature", "Fleeting", "Map"}, func(text string, index int) {

	})

	searchNoteType.SetLabel("Note Type: ").SetCurrentOption(0)

	app.SetInputCapture(func(key *tcell.EventKey) *tcell.EventKey {

		if key.Key() == tcell.KeyEscape {
			app.Stop()
			return nil
		}

    if key.Key() == tcell.KeyEnter {
      if searchResult.GetItemCount() == 0 {
        return nil
      }
      idx := searchResult.GetCurrentItem()

      selectedNote = &searchNotes[idx]

      app.Stop()
      return nil
    }

		if key.Key() == tcell.KeyTab {
			if searchField.HasFocus() {
				app.SetFocus(searchResult)
				return nil
			}

			if searchResult.HasFocus() {
				app.SetFocus(searchNoteType)
				return nil
			}

			if searchNoteType.HasFocus() {
				app.SetFocus(searchField)
				return nil
			}
		}

		return key
	})

	searchField.SetChangedFunc(func(text string) {
		searchNotes = searchInNotes(text, &allNotes, selectedNoteType)

		searchResult.Clear()

		for _, n := range searchNotes {
			searchResult.AddItem(n.Title, "", '\n', nil)
		}
	})

	grid := tview.NewGrid()
	grid.SetRows(1, 1, 0)
	grid.SetBorders(true)
	grid.SetMinSize(0, 0)
	grid.AddItem(searchField, 0, 0, 1, 1, 1, 40, true)
	grid.AddItem(searchNoteType, 1, 0, 1, 1, 1, 40, false)
	grid.AddItem(searchResult, 2, 0, 10, 1, 10, 40, false)

	if err := app.SetRoot(grid, true).Run(); err != nil {
		panic(err)
	}

	return selectedNote
}

func main() {
	flag.Parse()
	cmd := flag.Arg(0)

  noteHistory := make([]string, 0)

	if flag.NArg() < 2 {
		usage()
		return
	}

	switch cmd {
	case "search":
    result :=	searchUI(flag.Arg(1))

    if result == nil {
      fmt.Println("Nothing selected")
      return
    }

    noteHistory = append(noteHistory, result.Id)
    vresult := ViewExitReopen
    id := ""

    for {
      vresult, id = viewUI(openNoteData(*result))

      if vresult == ViewExitOk {
        break
      }

      if vresult == ViewExitOpen {
        *result = GetNoteRecordFromFile(id)
        noteHistory = append(noteHistory, result.Id)
      }

      if vresult == ViewExitFind {
        result = searchUI("")
        if result == nil {
          break
        }

        noteHistory = append(noteHistory, result.Id)
      }

      if vresult == ViewExitNew {
        *result = newNote("z")
        noteHistory = append(noteHistory, result.Id)
      }

      if vresult == ViewExitBack {
        if len(noteHistory) > 1 {
          noteHistory = noteHistory[0:len(noteHistory) - 1]
          *result = GetNoteRecordFromFile(noteHistory[len(noteHistory)-1])
        }
      }
    }

	case "new":
		newNote(flag.Arg(1))
	default:
		usage()
	}
}
