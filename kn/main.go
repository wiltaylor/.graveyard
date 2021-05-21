package main

import ( "fmt"; "flag"; "os"; )

func usage() {
  fmt.Println("kn command [options]")
  fmt.Println("")
  fmt.Println("Commands:")
  fmt.Println("new - Create a new note.")

}

func newNote() {
  notePath := os.Getenv("ZKDIR")
  if notePath == "" {
    notePath = "Empty"
  }

  fmt.Printf("%s", notePath)
}

func main() {
  flag.Parse()
  cmd := flag.Arg(0)


  switch(cmd) {
  case "new":
    newNote()
  default:
   usage()
  }
}
