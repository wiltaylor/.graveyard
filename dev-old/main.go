package main

import (
	"fmt"
	"os"
  "io/ioutil"
  "log"
  "strings"
  "os/exec"
  "errors"
  "path/filepath"
  "gopkg.in/yaml.v3"
)

type devFile struct {
  Actions []Action  `yaml:"actions"`
}

type Action struct {
  Name string `yaml:"name"`
  Description string `yaml:"description"`
  Command string `yaml:"command"`
}

func ReadActions(filename string) (devFile, error){
  
  var result devFile
  data, err := ioutil.ReadFile(filename)

  if(err != nil) {
    log.Fatal(err)
    return result, err
  }


  err = yaml.Unmarshal(data, &result)

  if(err != nil) {
    log.Fatal(err)
    return result, err
  }

  return result, nil
}

func main() {

	if len(os.Args) == 1 {
		usage()
		return
	}


	command := os.Args[1]
	args := make([]string, 0)

	if(len(os.Args) > 2) {
	  args = os.Args[2:]
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

func getActions() (devFile, error) {
  pwd, _ := os.Getwd()
  prjRoot, err := findPrjRoot(pwd)

  if err != nil {
    return devFile{}, err
  }

  if exists(filepath.Join(prjRoot, ".dev.yaml")) {
    return ReadActions(filepath.Join(prjRoot, ".dev.yaml"))
  }

  if exists(filepath.Join(prjRoot, ".git", "dev.yaml")) {

    return ReadActions(filepath.Join(prjRoot, ".git", "dev.yaml"))
  }

  return devFile{}, errors.New("Can't find dev.yaml file")

}

func initDev(args []string) {

  pwd, _ := os.Getwd()
  prjRoot, err := findPrjRoot(pwd)

  if err != nil {
    log.Fatal("You need to be in a git repo before calling init")
    return
  }


  if exists(filepath.Join(prjRoot, ".dev.yaml")) || exists(filepath.Join(prjRoot, ".git", "dev.yaml")) {
    log.Fatal("This project has already been inited")
    return
  }

  path := filepath.Join(prjRoot, ".dev.yaml")

  fmt.Printf("Path: %s", path)

  if len(args) == 1 && args[0] == "--hide" {
    path = filepath.Join(prjRoot, ".git", "dev.yaml")
  }


  f, err := os.Create(path)
  if err != nil {
    log.Fatal(err)
    return
  }

  fmt.Fprintln(f, "actions:")

  f.Close()
}

func prjHandler(args []string) {

  if len(args) == 0 {
    prjUsage()
    return
  }

  command := args[0]
  commandArgs := make([]string, 0)

  if len(args) > 1 {
    commandArgs = args[1:]
  }

  switch(command) {
    case "new":
      prjNew(commandArgs)
      break
    case "open":
      prjOpen(commandArgs)
      break
    case "info":
      prjInfo()
      break
    case "gen":
      generator(args)
    case "pwd":
      pwd, _ := os.Getwd()
      root, err := findPrjRoot(pwd)
      if err != nil {
       log.Fatal(err)
       return
      }

      fmt.Printf("%s\n", root)

      break
    default:
      prjUsage()
      break
  }
}

func parseProjectNameToGitUrl(name string) (string ,error) {
  parts := strings.Split(name, ":")

  if len(parts) != 2 {
    return "", errors.New("Invalid project name!")
  }

  provider := parts[0]

  projectParts := strings.Split(parts[1], "/")
  username := "wiltaylor"
  projectName := parts[1]

  if len(projectParts) > 1 {
    username = projectParts[0]
    projectName = projectParts[1]
  }

  switch(provider) {
    case "local":
      return "", nil
    case "gh":
      return fmt.Sprintf("git@github.com:%s/%s.git", username, projectName), nil
    case "gl":
      return fmt.Sprintf("git@gitlab.com:%s/%s.git", username, projectName), nil
    default:
      return "", errors.New("Unknown project provider type!")
  }
}

func parseProjectNameToPath(name string) (string, error) {
  parts := strings.Split(name, ":")

  if len(parts) != 2 {
    return "", errors.New("Invalid project name!")
  }

  provider := parts[0]

  projectParts := strings.Split(parts[1], "/")
  username := "wiltaylor"
  projectName := parts[1]

  if len(projectParts) > 1 {
    username = projectParts[0]
    projectName = projectParts[1]
  }

  switch(provider) {
    case "local":
      return fmt.Sprintf("%s/repo/local/%s/%s", os.Getenv("HOME"), username, projectName), nil
    case "gh":
      return fmt.Sprintf("%s/repo/github.com/%s/%s", os.Getenv("HOME"), username, projectName), nil
    case "gl":
      return fmt.Sprintf("%s/repo/gitlab.com/%s/%s", os.Getenv("HOME"), username, projectName), nil
    default:
      return "", errors.New("Unknown project provider type!")
  }
}

func generator(args []string) {
  if len(args) < 1 {
    log.Fatal("Expected a generator template name")
    return
  }

  pwd, _ := os.Getwd()
  prjRoot, err := findPrjRoot(pwd)

  if err != nil {
    log.Fatal("You need to be in a git repo before calling gen")
    return
  }
  generator := args[0]
  generatorArgs := make([]string, 0)
  generatorDir := filepath.Join(os.Getenv("WTDEV"), "generators")

  if generatorDir == "" {
    log.Fatal("WTDEV environment variable is not set!")
    return
  }

  if len(args) > 2 {
    generatorArgs = args[1:]
  }

  command := fmt.Sprintf("%s/%s %s", generatorDir, generator, strings.Join(generatorArgs, " "))
  
  err = execute(command, prjRoot)

  if err != nil {
    log.Fatal(err)
  }
}

func prjNew(args []string) {
  if len(args) < 2 {
    log.Fatal("Expected a project name and template name to be supplied")
    return
  }

  projectPath, err := parseProjectNameToPath(args[0])

  if err != nil {
    log.Fatal(err)
    return
  }

  if exists(projectPath) {
    log.Fatal("Can't create project at that path because it already exists!")
    return
  }

  err = os.Mkdir(projectPath, 0755)

  if err != nil {
    log.Fatal(err)
    return
  }

  template := args[1]
  templateArgs := make([]string, 0)
  templateDir := filepath.Join(os.Getenv("WTDEV"), "templates")

  if templateDir == "" {
    log.Fatal("WTDEV environment variable is not set!")
    return
  }

  if len(args) > 2 {
    templateArgs = args[1:]
  }

  command := fmt.Sprintf("%s/%s %s", templateDir, template, strings.Join(templateArgs, " "))

  cmd := exec.Command("sh", "-c", command)
  cmd.Dir = projectPath
  cmd.Stderr = os.Stderr
  cmd.Stdout = os.Stdout
  cmd.Stdin = os.Stdin

  err = cmd.Run()

  if err != nil {
    log.Fatal(err)
  }
}

func prjOpen(args []string) {
  if len(args) < 1 {
    log.Fatal("Expected a project name")
    return
  }

  projectPath, err := parseProjectNameToPath(args[0])

  if err != nil {
    log.Fatal(err)
    return
  }

  gitUrl, err := parseProjectNameToGitUrl(args[0])

  fmt.Printf("Git url: %s", gitUrl)

  if !exists(projectPath) {
    
    if gitUrl == "" {
      log.Fatal("You have specified a local repository that doesn't exist!")
      return
    }

    parentDir := filepath.Dir(projectPath)
    os.Mkdir(parentDir, 0755)

    err = execute(fmt.Sprintf("git clone %s %s", gitUrl,projectPath), parentDir)

    if err != nil {
      log.Fatal(err)
    }

    return
  }

  fmt.Println(projectPath)
}

func prjInfo() {
  pwd, _ := os.Getwd()
  prjRoot, err := findPrjRoot(pwd)

  if err != nil {
    fmt.Println("You don't appear to be in a project")
    return
  }

  if exists(filepath.Join(prjRoot, "README.md")) {
    execute(fmt.Sprintf("cat %s", filepath.Join(prjRoot, "README.md")),"")
  }
}

func prjUsage() {
  fmt.Println("dev prj Usage:")
  fmt.Println("dev prj {command}")
  fmt.Println("")
  fmt.Println("Commands:")
  fmt.Println("new - Create new project")
  fmt.Println("open - Opens a project")
  fmt.Println("info - Shows info on the current project")
  fmt.Println("pwd - Shows the current project root directory")
}

func findPrjRoot(path string) (string, error) {
   if path == "" || path == "/" {
     return "", errors.New("Can't find project root. Are you in a project directory.")
   }


   if exists(filepath.Join(path, ".git")) {
     return path, nil
   }

   dir := filepath.Dir(path)
   return findPrjRoot(dir)
}

func listAction() {

  dev, err := getActions()

  if err != nil {
    log.Fatal(err)
    return
  }

  actions := dev.Actions

  fmt.Println("Actions:")

  for _, item := range actions {
    fmt.Printf(" - [%s] - %s\n", item.Name, item.Description)
  }

  fmt.Println("")
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
  fmt.Println("")
}
