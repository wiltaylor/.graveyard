package main

import (
	"errors"
	"fmt"
	"gopkg.in/yaml.v2"
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
)

type devFile struct {
	Actions []Action `yaml:"actions"`
}

type Action struct {
	Name        string `yaml:"name"`
	Description string `yaml:"description"`
	Command     string `yaml:"command"`
}

func ReadActions(filename string) (devFile, error) {

	var result devFile
	data, err := ioutil.ReadFile(filename)

	if err != nil {
		log.Fatal(err)
		return result, err
	}

	err = yaml.Unmarshal(data, &result)

	if err != nil {
		log.Fatal(err)
		return result, err
	}

	return result, nil
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
