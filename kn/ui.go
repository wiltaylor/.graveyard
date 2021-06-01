package main

import (
	"bufio"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"

	"fmt"

	"github.com/gdamore/tcell/v2"
	"github.com/rivo/tview"
	"golang.design/x/clipboard"
)

const (
	LinkIconUrl        = ""
	LinkIconEmpty      = ""
	LinkIconNote       = ""
	LinkIconAttachment = ""
	LinkIconReport     = ""
)

type ViewMode int

const (
	ViewModeMain ViewMode = iota
	ViewModeSearch
	ViewModeSearchLink
)

// Main screen
var app *tview.Application
var toolbar *tview.TextView
var textbox *tview.TextView
var mainLayout *tview.Grid

// Search screen
var searchLayout *tview.Grid
var searchField *tview.InputField
var searchResult *tview.Table
var typeForm *tview.Form
var zettleCheck *tview.Checkbox

// Search Checkboxes
var zettleChk, mapChk, litChk, fleetingChk bool

var CurrentViewMode ViewMode
var CurrentSearchResults []NoteHeader
var CurrentNote NoteData
var CurrentSearchSelection int
var CurrentLinkIndex int
var NoteHistory []string

func InitUI() {
	app = tview.NewApplication()

	CurrentViewMode = ViewModeMain

	// Main view controls
	toolbar = tview.NewTextView()
	toolbar.SetText("ESC-Quit|N-New|F-Find|E-Edit|A-AddLink|D-DeleteNote|HJKL-Move|Enter-FollowLink|Backspace-Back|F1-Dashboard|F10-Sync")
	toolbar.SetBackgroundColor(tcell.ColorWhite)
	toolbar.SetTextColor(tcell.ColorBlack)

	textbox = tview.NewTextView()
	textbox.SetText("")
	textbox.SetScrollable(true)
	textbox.SetTitle("Empty")
	textbox.SetBorder(true)
	textbox.SetBorderPadding(0, 0, 1, 1)
	textbox.SetDynamicColors(true)
	textbox.SetRegions(true)
	CurrentLinkIndex = -1

	mainLayout = tview.NewGrid()
	mainLayout.SetRows(1, 5, 0)
	mainLayout.AddItem(toolbar, 0, 0, 1, 1, 1, 1, false)
	mainLayout.AddItem(textbox, 1, 0, 10, 1, 10, 40, true)

	// Search Window controls
	searchField = tview.NewInputField()
	searchField.SetText("")
	searchField.SetChangedFunc(SearchUpdate)
	searchField.SetLabel("Note Title: ")

	typeForm = tview.NewForm()
	typeForm.AddCheckbox("Zettle", true, func(checked bool) {
		zettleChk = checked
	})
	zettleChk = true

	typeForm.AddCheckbox("Map", true, func(checked bool) {
		mapChk = checked
	})
	mapChk = true

	typeForm.AddCheckbox("Literature", false, func(checked bool) {
		litChk = checked
	})
	litChk = false
	typeForm.AddCheckbox("Fleeting", false, func(checked bool) {
		fleetingChk = checked
	})
	fleetingChk = false

	typeForm.SetHorizontal(true)
	typeForm.SetBorder(true)

	searchResult = tview.NewTable()
	searchResult.SetSelectable(true, false)
	searchResult.SetSelectionChangedFunc(func(row int, _ int) {
		CurrentSearchSelection = row
	})

	searchLayout = tview.NewGrid()
	searchLayout.SetRows(1, 1, 0)
	searchLayout.SetMinSize(0, 0)
	searchLayout.AddItem(searchField, 0, 0, 1, 1, 1, 1, true)
	searchLayout.AddItem(typeForm, 2, 0, 1, 1, 1, 40, false)
	searchLayout.AddItem(searchResult, 3, 0, 10, 1, 10, 40, false)

	app.SetInputCapture(handleInput)
	app.SetRoot(mainLayout, true)
	app.SetFocus(textbox)

	NoteHistory = make([]string, 0)

	CurrentNote = DashboardReport()
	RefreshFileView()
}

func SwitchView(mode ViewMode) {
	switch mode {
	case ViewModeMain:
		app.SetRoot(mainLayout, true)
		break
	case ViewModeSearch:
		app.SetRoot(searchLayout, true)
		app.SetFocus(searchField)
		SearchUpdate(searchField.GetText())
		break
	case ViewModeSearchLink:
		app.SetRoot(searchLayout, true)
		app.SetFocus(searchField)
		SearchUpdate(searchField.GetText())
		break
	}

	CurrentViewMode = mode
}

func SearchUpdate(txt string) {
	typs := make([]NoteType, 0)

	if zettleChk {
		typs = append(typs, ZettleNote)
	}

	if mapChk {
		typs = append(typs, MapNote)
	}

	if litChk {
		typs = append(typs, LiteratureNote)
	}

	if fleetingChk {
		typs = append(typs, FleetingNote)
	}

	CurrentSearchResults = FindNotes(txt, typs)

	searchResult.Clear()

	for idx, item := range CurrentSearchResults {
		searchResult.SetCellSimple(idx, 0, item.Title)
	}

	if len(CurrentSearchResults)-1 > CurrentSearchSelection {
		CurrentSearchSelection = 0
	}
}

func handleInput(event *tcell.EventKey) *tcell.EventKey {

	if CurrentViewMode == ViewModeMain {
		if event.Key() == tcell.KeyF10 {

			app.Suspend(func() {
				if DoDataSync() == false {
					fmt.Println("Press enter key to continue...")
					bufio.NewReader(os.Stdin).ReadBytes('\n')
				}
			})

		}

		if event.Key() == tcell.KeyF1 {
			CurrentNote = OpenReport("rp:dashboard")
			RefreshFileView()
			return nil
		}

		if event.Key() == tcell.KeyEsc {
			ShutdownUI()
			return nil
		}

		if event.Key() == tcell.KeyBackspace || event.Key() == tcell.KeyBackspace2 {
			if len(NoteHistory) <= 1 {
				return nil
			}

			id := NoteHistory[len(NoteHistory)-2]

			NoteHistory = NoteHistory[0 : len(NoteHistory)-2]

			head, err := GetHeaderFromFile(id)

			if err != nil {
				panic(err)
			}

			d, err := GetNoteData(head)

			if err != nil {
				panic(err)
			}

			CurrentNote = d
			RefreshFileView()
			return nil
		}

		if event.Key() == tcell.KeyEnter {
			if CurrentLinkIndex == -1 {
				return nil
			}

			if len(CurrentNote.Links) == 0 {

				return nil
			}

			lnk := CurrentNote.Links[CurrentLinkIndex]

			if lnk.Type == LinkUrl {
				cmd := exec.Command("xdg-open", lnk.Path)
				cmd.Start()

				return nil
			}

			if lnk.Type == LinkAttachment {
				if len(lnk.Path) <= 4 {
					return nil
				}

				path := filepath.Join(NoteDirectory, ".attachments", lnk.Path[4:])

				cmd := exec.Command("xdg-open", path)
				cmd.Start()
				return nil
			}

			if lnk.Type == LinkReport {
				CurrentNote = OpenReport(lnk.Path)
				RefreshFileView()
				return nil
			}

			if lnk.Type == LinkNote {
				id := lnk.Path[3:]
				h, err := GetHeaderFromFile(id)

				if err != nil {

					return nil
				}

				n, err := GetNoteData(h)

				if err != nil {

					return nil
				}

				CurrentNote = n
				NoteHistory = append(NoteHistory, CurrentNote.Header.Id)
				RefreshFileView()

			}

			return nil
		}

		if event.Key() == tcell.KeyTab {
			if len(CurrentNote.Links) == 0 {
				return nil
			}

			CurrentLinkIndex++

			if CurrentLinkIndex >= len(CurrentNote.Links) {
				CurrentLinkIndex = 0
			}

			textbox.Highlight(fmt.Sprintf("%v", CurrentNote.Links[CurrentLinkIndex].Id))
			textbox.ScrollToHighlight()

			return nil
		}

		if event.Rune() == 'f' {
			SwitchView(ViewModeSearch)
			return nil
		}

		if event.Rune() == 'e' {
			if CurrentNote.Header.Filename != "" {
				EditFile(CurrentNote.Header.Filename)
				RefreshNote(CurrentNote.Header.Id)
				RefreshFileView()
			}
			return nil
		}

		if event.Rune() == 'a' {
			SwitchView(ViewModeSearchLink)
			return nil
		}

		if event.Rune() == 'd' {
			RemoveNote(CurrentNote.Header.Id)
			CurrentNote.Header.Id = ""
			CurrentNote.Header.Filename = ""
			CurrentNote.Header.Title = "Empty"
			CurrentNote.RawText = ""
			CurrentNote.Links = make([]NoteLink, 0)

			textbox.SetText("")
			textbox.ScrollToBeginning()
			textbox.SetTitle("Empty")

			return nil
		}

		if event.Rune() == 'n' {
			note, err := NewNote("New Note", ZettleNote)

			NoteHistory = append(NoteHistory, note.Header.Id)

			if err != nil {
				panic(err)
			}

			CurrentNote = note
			EditFile(CurrentNote.Header.Filename)
			RefreshNote(CurrentNote.Header.Id)
			RefreshFileView()
		}
	}

	if CurrentViewMode == ViewModeSearch || CurrentViewMode == ViewModeSearchLink {
		if event.Key() == tcell.KeyEsc {
			SwitchView(ViewModeMain)
			return nil
		}

		if searchResult.HasFocus() && event.Rune() == 'c' {
			note := CurrentSearchResults[CurrentSearchSelection]
			clipboard.Write(clipboard.FmtText, []byte(note.Id))

			return nil
		}

		if event.Key() == tcell.KeyLeft {
			idx := 0

			for {
				if idx >= typeForm.GetFormItemCount() {
					return nil
				}

				ctrl := typeForm.GetFormItem(idx)
				if ctrl.HasFocus() {

					if idx == 0 {
						app.SetFocus(typeForm.GetFormItem(typeForm.GetFormItemCount() - 1))
						return nil
					}

					app.SetFocus(typeForm.GetFormItem(idx - 1))
					return nil
				}
				idx++
			}
		}

		if event.Key() == tcell.KeyRight {
			idx := 0

			for {
				if idx >= typeForm.GetFormItemCount() {
					return event
				}

				ctrl := typeForm.GetFormItem(idx)
				if ctrl.HasFocus() {

					if idx == typeForm.GetFormItemCount()-1 {
						app.SetFocus(typeForm.GetFormItem(0))
						return nil
					}

					app.SetFocus(typeForm.GetFormItem(idx + 1))
					return nil
				}

				idx++
			}
		}

		if event.Key() == tcell.KeyEnter {
			if len(CurrentSearchResults) == 0 {
				return nil
			}

			if CurrentViewMode == ViewModeSearchLink {
				note := CurrentSearchResults[CurrentSearchSelection]
				CurrentNote.RawText += fmt.Sprintf("\n[%s](zk:%s)\n", note.Title, note.Id)
				SaveNoteData(CurrentNote)
			} else {

				n, err := GetNoteData(CurrentSearchResults[CurrentSearchSelection])

				if err != nil {
					return nil
				}

				CurrentNote = n
				NoteHistory = append(NoteHistory, CurrentNote.Header.Id)
			}
			RefreshFileView()
			SwitchView(ViewModeMain)
		}

		if event.Key() == tcell.KeyTab {
			if searchField.HasFocus() {
				app.SetFocus(searchResult)
				return nil
			}

			if searchResult.HasFocus() {
				app.SetFocus(typeForm)
				return nil
			}

			if typeForm.HasFocus() {
				app.SetFocus(searchField)
				return nil
			}
			return nil
		}

		if event.Key() == tcell.KeyF5 {
			RefreshNotes()
			SearchUpdate(searchField.GetText())
		}
	}

	return event
}

func ShutdownUI() {
	app.Stop()
	app = nil
}

func RunUI() {
	if err := app.Run(); err != nil {
		panic(err)
	}
}

func RefreshFileView() {

	if CurrentNote.Header.Type != ReportNote {
		note, err := GetNoteData(CurrentNote.Header)

		if err != nil {
			panic(err)
		}
		CurrentNote = note
	}

	FormatCurrentFile(&CurrentNote)

	textbox.SetText(CurrentNote.FormatedText)
	textbox.SetTitle(CurrentNote.Header.Title)
	textbox.Highlight()
	textbox.ScrollToBeginning()
	CurrentLinkIndex = -1
}

func EditFile(filename string) {
	editor := os.Getenv("EDITOR")

	if editor == "" {
		editor = "vim"
	}

	app.Suspend(func() {
		cmd := exec.Command(editor, filename)
		cmd.Stderr = os.Stderr
		cmd.Stdout = os.Stdout
		cmd.Stdin = os.Stdin
		err := cmd.Run()

		if err != nil {
			panic(err)
		}
	})

}

func FormatCurrentFile(file *NoteData) {
	text := file.RawText

	h6 := regexp.MustCompile(`###### (.+)\n`)
	h5 := regexp.MustCompile(`##### (.+)\n`)
	h4 := regexp.MustCompile(`#### (.+)\n`)
	h3 := regexp.MustCompile(`### (.+)\n`)
	h2 := regexp.MustCompile(`## (.+)\n`)
	h1 := regexp.MustCompile(`# (.+)\n`)
	b1 := regexp.MustCompile(`\n [-|*|+] `)
	b2 := regexp.MustCompile(`\n   [-|*|+] `)
	b3 := regexp.MustCompile(`\n     [-|*|+] `)
	l1 := regexp.MustCompile(`\n ([0-9]{1,5})\. `)
	l2 := regexp.MustCompile(`\n   ([0-9a-z]{1,5})\. `)
	l3 := regexp.MustCompile(`\n     ([0-9a-z]{1,5})\. `)
	codefence := regexp.MustCompile("(?s)\n```\n(.+)\n```\n")

	for _, lnk := range file.Links {
		ico := LinkIconUrl

		if lnk.Type == LinkNote {
			ico = LinkIconNote
		}

		if lnk.Type == LinkAttachment {
			ico = LinkIconAttachment
		}

		if strings.Trim(lnk.Path, " ") == "" {
			ico = LinkIconEmpty
		}

		if lnk.Type == LinkReport {
			ico = LinkIconReport
		}

		lnkText := fmt.Sprintf("[%s](%s)", lnk.Title, lnk.Path)
		newText := fmt.Sprintf("[\"%v\"]%s[blue::u]%s[-:-:-][\"\"]", lnk.Id, ico, lnk.Title)
		text = strings.Replace(text, lnkText, newText, 1)
	}

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

	text = codefence.ReplaceAllString(text, "\n[green:gray]$1[-:-:-]\n")

	file.FormatedText = text
}
