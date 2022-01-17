package main

import (
	"fmt"
	"os"
  "log"
  "strings"
  "path/filepath"
)

func main() {

	if len(os.Args) == 1 {
		usage()
		return
	}

  command, args, err := getCommandAndArgs(os.Args[1:])

  if err != nil {
    usage()
  }

	switch command {
	case "prj":
    prjHandler(args)
		break
  case "init":
    initDev(args)
    break
  case "ls":
    listAction()
    break;
  case "gen":
    generator(args)
	default:
    doAction(command, args)
		break
	}
}

func getGlobalPath() string {

  result := os.Getenv("WTDEV")

  if result == "" {

    homeDir := os.Getenv("HOME")
    result = fmt.Sprintf("%s/.local/share/wtdev", homeDir)
    
    if !exists(result) {
      os.Mkdir(result, 0755)
    }
  }

  return result
}

func generator(args []string) {
  pwd, _ := os.Getwd()
  prjRoot, err := findPrjRoot(pwd)

  if err != nil {
    log.Fatal("You need to be in a git repo before calling gen")
    return
  }

  generator, generatorArgs, err := getCommandAndArgs(args)

  if err != nil {
    log.Fatal("Expected a generator template name")
    return
  }

  generatorDir := filepath.Join(getGlobalPath(), "generators")

  if generatorDir == "" {
    log.Fatal("WTDEV environment variable is not set!")
    return
  }

  command := fmt.Sprintf("%s/%s %s", generatorDir, generator, strings.Join(generatorArgs, " "))
  
  err = execute(command, prjRoot)

  if err != nil {
    log.Fatal(err)
  }
}

func doAction(name string, args []string) {
  dev, err := getActions()

  if err != nil {
    log.Fatal(err)
    return
  }

  actions := dev.Actions
  for _, item := range actions {
    if item.Name == name {
      command := item.Command

      for i := 0; i < len(args); i++ {
        command = strings.Replace(command, fmt.Sprintf("%%%d", i + 1), args[i], -1)
      }

      command = strings.Replace(command, "%*", strings.Join(args, " "), -1)
      err := execute(command, "")

      if err != nil {
        log.Fatal(err)
      }
    }
  }
}

func usage() {
	fmt.Println("dev Usage:")
	fmt.Println("dev {command} [Options] {command args}")
	fmt.Println("")
	fmt.Println("Commands:")
  fmt.Println("prj [--hide] - Project managment commands")
  fmt.Println("init - Add dev config to a project")
  fmt.Println("gen {template name} - Use generator to add something to project")
}
