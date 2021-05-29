package main

import ()

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

  return DashboardReport()
}
