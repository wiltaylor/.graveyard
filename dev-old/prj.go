package main

import (
	"errors"
	"fmt"
	"log"
	"os"
	"strings"

	"path/filepath"
)

func prjHandler(args []string) {

	command, commandArgs, err := getCommandAndArgs(args)

	if err != nil {
		prjUsage()
		return
	}

	switch command {
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

func parseProjectNameToGitUrl(name string) (string, error) {
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

	switch provider {
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

	switch provider {
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

	template, templateArgs, err := getCommandAndArgs(args)

	if err != nil {
		log.Fatal("Expected a project name and template name to be supplied")
		return
	}

	templateDir := filepath.Join(getGlobalPath(), "templates")

	if templateDir == "" {
		log.Fatal("WTDEV environment variable is not set!")
		return
	}

	if len(args) > 2 {
		templateArgs = args[1:]
	}

	command := fmt.Sprintf("%s/%s %s", templateDir, template, strings.Join(templateArgs, " "))
	err = execute(command, projectPath)

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

		err = execute(fmt.Sprintf("git clone %s %s", gitUrl, projectPath), parentDir)

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
		execute(fmt.Sprintf("cat %s", filepath.Join(prjRoot, "README.md")), "")
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
