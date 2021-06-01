package main

import (
	"fmt"
)

func DashboardReport() NoteData {
	logo :=
		`
KKKKKKKKK    KKKKKKKNNNNNNNN        NNNNNNNN
K:::::::K    K:::::KN:::::::N       N::::::N
K:::::::K    K:::::KN::::::::N      N::::::N
K:::::::K   K::::::KN:::::::::N     N::::::N
KK::::::K  K:::::KKKN::::::::::N    N::::::N
  K:::::K K:::::K   N:::::::::::N   N::::::N
  K::::::K:::::K    N:::::::N::::N  N::::::N
  K:::::::::::K     N::::::N N::::N N::::::N
  K:::::::::::K     N::::::N  N::::N:::::::N
  K::::::K:::::K    N::::::N   N:::::::::::N
  K:::::K K:::::K   N::::::N    N::::::::::N
KK::::::K  K:::::KKKN::::::N     N:::::::::N
K:::::::K   K::::::KN::::::N      N::::::::N
K:::::::K    K:::::KN::::::N       N:::::::N
K:::::::K    K:::::KN::::::N        N::::::N
KKKKKKKKK    KKKKKKKNNNNNNNN         NNNNNNN

# Map of Content
`

tagged := FindByTag("dashboard", []NoteType{MapNote})

for _, note := range tagged {
  logo += fmt.Sprintf(" - [%s](zk:%s)\n", note.Title, note.Id)
}

logo += `
# Reports
 - [Literature Notes](rp:literature)
 - [Fleeting Notes](rp:fleeting)
 - [Unknown Notes](rp:unknown)
 - [New Zettles](rp:newzettle)
`

	header := NoteHeader{Title: "Dashboard", Id: "", Type: ReportNote, Filename: "", Date: "", State: NewState}
	result := NoteData{Header: header, RawText: logo, FormatedText: "", Links: make([]NoteLink, 0)}

	ExtractLinks(&result)
	return result
}

func OpenReport(path string) NoteData {

	if path == "rp:dashboard" {
		return DashboardReport()
	}

	if path == "rp:literature" {
		return LiteratureNoteReport()
	}

	if path == "rp:fleeting" {
		return FleetingNotesReport()
	}

	if path == "rp:newzettle" {
		return NewZettleNotesReport()
	}

	if path == "rp:unknown" {
		return UnknownNotes()
	}

	return DashboardReport()
}

func FleetingNotesReport() NoteData {
	notes := FindNotes("", []NoteType{FleetingNote})

	text := "# Fleeting Notes:\n"

	for _, n := range notes {
		if n.State != DoneState {
			text += fmt.Sprintf(" - [%s](zk:%s)\n", n.Title, n.Id)
		}
	}

	header := NoteHeader{Title: "Fleeting Notes", Id: "", Type: ReportNote, Filename: "", Date: "", State: NewState}
	result := NoteData{Header: header, RawText: text, FormatedText: "", Links: make([]NoteLink, 0)}

	ExtractLinks(&result)
	return result
}

func UnknownNotes() NoteData {

	notes := FindNotes("", []NoteType{UnknownNote})

	text := "# Unknown Note Types:\n"

	for _, n := range notes {
		text += fmt.Sprintf(" - [%s](zk:%s)\n", n.Title, n.Id)
	}

	header := NoteHeader{Title: "Unknown Notes", Id: "", Type: ReportNote, Filename: "", Date: "", State: NewState}
	result := NoteData{Header: header, RawText: text, FormatedText: "", Links: make([]NoteLink, 0)}

	ExtractLinks(&result)
	return result

}

func NewZettleNotesReport() NoteData {
	notes := FindNotes("", []NoteType{ZettleNote})

	text := "# New Zettle Notes:\n"

	for _, n := range notes {
		if n.State == NewState {
			text += fmt.Sprintf(" - [%s](zk:%s)\n", n.Title, n.Id)
		}
	}

	text += "\n# Unknown State Notes:\n"
	for _, n := range notes {
		if n.State != NewState && n.State != GreenState {
			text += fmt.Sprintf(" - [%s](zk:%s)\n", n.Title, n.Id)
		}
	}

	header := NoteHeader{Title: "Unknown Notes", Id: "", Type: ReportNote, Filename: "", Date: "", State: NewState}
	result := NoteData{Header: header, RawText: text, FormatedText: "", Links: make([]NoteLink, 0)}

	ExtractLinks(&result)
	return result
}

func LiteratureNoteReport() NoteData {

	litNotes := FindNotes("", []NoteType{LiteratureNote})

	text := "# Ready Notes\n"

	for _, n := range litNotes {
		if n.State == ReadyState {
			text += fmt.Sprintf(" - [%s](zk:%s)\n", n.Title, n.Id)
		}
	}

	text += "\n# New Notes\n"
	for _, n := range litNotes {
		if n.State == NewState {
			text += fmt.Sprintf(" - [%s](zk:%s)\n", n.Title, n.Id)
		}
	}

	text += "\n# Unknown Notes\n"
	for _, n := range litNotes {
		if n.State == UnknownState {
			text += fmt.Sprintf(" - [%s](zk:%s)\n", n.Title, n.Id)
		}
	}

	header := NoteHeader{Title: "Literature Notes", Id: "", Type: ReportNote, Filename: "", Date: "", State: NewState}
	result := NoteData{Header: header, RawText: text, FormatedText: "", Links: make([]NoteLink, 0)}

	ExtractLinks(&result)

	return result
}
