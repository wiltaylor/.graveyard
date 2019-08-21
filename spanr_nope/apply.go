package main

import (
	"errors"
	"fmt"
	"io/ioutil"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"

	"gopkg.in/yaml.v2"
)

func listConfig(path string) error {

	absPath, _ := filepath.Abs(path)

	config := absPath + "/config.yaml"

	//Load gatherers

	printGatherers(path)

	//Load resources
	res, err := loadResources(absPath)

	if err != nil {
		fmt.Println("Failed to load resources")
		return err
	}

	for _, r := range res {
		fmt.Printf("Name: %v\n", r.Name)
		fmt.Printf("Description: %v\n", r.Description)
		fmt.Printf("Author: %v\n", r.Author)
		fmt.Printf("Version: %v\n", r.Version)
		fmt.Printf("TestCommand: %v\n", r.TestCommand)
		fmt.Printf("ApplyCommand: %v\n", r.ApplyCommand)
		fmt.Printf("TestArguments: %v\n", r.TestArguments)
		fmt.Printf("ApplyArguments: %v\n", r.ApplyArguments)
		fmt.Printf("Properties:\n")

		for key, val := range r.Properties {
			fmt.Printf("  %v = %v\n", key, val)
		}
	}

	//Load config
	cfg, err := loadConfig(config)

	if err != nil {
		fmt.Println("Failed to load config!")
		return err
	}

	fmt.Printf("Name: %v\n", cfg.Name)
	fmt.Printf("Author: %v\n", cfg.Author)
	fmt.Printf("Version: %v\n", cfg.Version)
	fmt.Printf("Description: %v\n", cfg.Description)
	fmt.Printf("Condition: %v\n", cfg.Condition)
	fmt.Printf("Config Items:\n")

	for _, item := range cfg.Items {
		fmt.Printf(" - Name: %v\n", item.Name)
		fmt.Printf("   Resource: %v\n", item.Resource)
		fmt.Printf("   Options: \n")

		for key, val := range item.Options {
			fmt.Printf("      %v = %v\n", key, val)
		}

		fmt.Println()
	}

	return nil

}

func runConfig(path string, props string, test bool, config string) (int, ConfigInfo) {

	absPath, _ := filepath.Abs(path)

	if config == "" {
		config = absPath + "/config.yaml"
	}

	//Add runtimes to path
	err := loadRuntimes(absPath)

	if err != nil {
		fmt.Println("Failed to load runtimes!")
		return CFGError, ConfigInfo{}
	}

	//Get properties and apply it to environment
	err = loadProperties(props)

	if err != nil {
		fmt.Println("Failed to load properties!")
		return CFGError, ConfigInfo{}
	}

	//Load gatherers
	err = loadGatherers(absPath)

	if err != nil {
		fmt.Println("Failed to load gatherers!")
		return CFGError, ConfigInfo{}
	}

	//Load resources
	res, err := loadResources(absPath)

	if err != nil {
		fmt.Println("Failed to load resources")
		return CFGError, ConfigInfo{}
	}

	//Load config
	cfg, err := loadConfig(config)

	if err != nil {
		fmt.Println("Failed to load config!")
		return CFGError, ConfigInfo{}
	}

	//Process config

	for i, item := range cfg.Items {
		state := processConfig(item, test, res)
		cfg.Items[i].State = state

		if !test {
			if state == CFGRebootRequired {
				fmt.Println("Requires reboot")
				return CFGRebootRequired, cfg
			}

			if state == CFGError {
				fmt.Println("Error state!")
				return CFGError, cfg
			}
		}

	}

	return CFGConfigured, cfg
}

func processConfig(item ConfigItem, test bool, resources []ResourceInfo) int {

	resource, err := findResource(item.Resource, resources)

	if err != nil {
		fmt.Println("Can't find resource!")
		return CFGError
	}

	if !test && item.Condition != "" {
		if item.Condition[0] == '!' {
			if os.Getenv(item.Condition[1:]) != "" {
				return CFGSkipOnDep
			}
		} else {
			if os.Getenv(item.Condition) == "" {
				return CFGSkipOnDep
			}
		}
	}

	state := runTest(item, resource)

	if test || state == CFGConfigured || state == CFGError || state == CFGRebootRequired || state == CFGNotRun {
		return state
	}

	applyState := runApply(item, resource)

	if applyState == CFGNotRun || applyState == CFGRebootRequired || applyState == CFGError || applyState == CFGNotConfigured {
		return applyState
	}

	state = runTest(item, resource)

	if state == CFGNotConfigured {
		return CFGError
	}

	return state
}

func findResource(name string, resources []ResourceInfo) (ResourceInfo, error) {
	for _, r := range resources {
		if r.Name == name {
			return r, nil
		}
	}

	return ResourceInfo{}, errors.New("can't find resource")
}

func runTest(config ConfigItem, resource ResourceInfo) int {
	for key, val := range config.Options {
		os.Setenv(key, val)
	}

	currentDir, _ := os.Getwd()
	os.Chdir(resource.Path)

	cmd := exec.Command(resource.TestCommand, resource.TestArguments...)

	result, err := cmd.CombinedOutput()
	if err != nil {
		fmt.Printf("Failed to test resource\nStd:%v\nError: %v\n", result, err)
		return CFGError
	}

	ret := getStateFromString(string(result))
	msgs := getMessagesFromStd(string(result))

	for _, msg := range msgs {
		fmt.Printf("MSG: %v\n", msg)
	}

	for key := range config.Options {
		os.Unsetenv(key)
	}

	os.Chdir(currentDir)

	return ret
}

func runApply(config ConfigItem, resource ResourceInfo) int {
	for key, val := range config.Options {
		os.Setenv(key, val)
	}

	currentDir, _ := os.Getwd()
	os.Chdir(resource.Path)

	cmd := exec.Command(resource.ApplyCommand, resource.ApplyArguments...)

	result, err := cmd.CombinedOutput()
	if err != nil {
		fmt.Printf("Failed to test resource\nStd:%v\nError: %v\n", result, err)
		return CFGError
	}

	ret := getStateFromString(string(result))

	for key := range config.Options {
		os.Unsetenv(key)
	}

	os.Chdir(currentDir)

	return ret
}

func getStateFromString(text string) int {

	vars := getVarsFromStd(text)

	for key, val := range vars {
		os.Setenv(key, val)
		fmt.Printf("Setting Var %v = %v\n", key, val)
	}

	if strings.Index(text, "##FAIL##") != -1 {
		return CFGError
	}

	if strings.Index(text, "##CONFIGURED##") != -1 {
		return CFGConfigured
	}

	if strings.Index(text, "##REBOOT##") != -1 {
		return CFGRebootRequired
	}

	if strings.Index(text, "##NOTCONFIGURED##") != -1 {
		return CFGNotConfigured
	}

	return CFGError
}

func loadConfig(path string) (ConfigInfo, error) {
	file, err := os.Open(path)

	if err != nil {
		fmt.Println("Failed to load config.yaml file!")
		return ConfigInfo{}, err
	}

	fileInfo, err := file.Stat()

	if err != nil {
		fmt.Println("Failed to get file size of config.yaml!")
		return ConfigInfo{}, err
	}

	data := make([]byte, fileInfo.Size())

	_, err = file.Read(data)

	if err != nil {
		fmt.Println("Failed to read contents of config.yaml file")
		return ConfigInfo{}, err
	}

	var config ConfigInfo

	err = yaml.Unmarshal(data, &config)

	if err != nil {
		fmt.Println("Failed to parse config.yaml file!")
		fmt.Printf("Error: %v", err)
		return ConfigInfo{}, err
	}

	file.Close()

	return config, nil

}

func loadResources(path string) ([]ResourceInfo, error) {
	dirs, err := ioutil.ReadDir(path + "/resources")
	resources := make([]ResourceInfo, 0)

	if err != nil {
		fmt.Printf("Unable to open resources folder!")
		return nil, err
	}

	for _, d := range dirs {
		if !d.IsDir() {
			continue
		}

		resourceName := d.Name()

		file, err := os.Open(path + "/resources/" + resourceName + "/resource.yaml")

		if err != nil {
			fmt.Println("Failed to load resource.yaml file!")
			return nil, err
		}

		fileInfo, err := file.Stat()

		if err != nil {
			fmt.Println("Failed to get file size of resource.yaml!")
			return nil, err
		}

		data := make([]byte, fileInfo.Size())

		_, err = file.Read(data)

		if err != nil {
			fmt.Println("Failed to read contents of resource.yaml file")
			return nil, err
		}

		var res []ResourceInfo

		err = yaml.Unmarshal(data, &res)

		if err != nil {
			fmt.Println("Failed to parse resource.yaml file!")
			fmt.Printf("Error: %v", err)
			return nil, err
		}

		file.Close()

		for i := range res {
			res[i].Path = path + "/resources/" + resourceName
		}

		resources = append(resources, res...)
	}

	return resources, nil
}

func printGatherers(path string) error {
	dirs, err := ioutil.ReadDir(path + "/gathers")

	if err != nil {
		fmt.Println("Failed to find any gatherers!")
		return err
	}

	fmt.Println("Gatherers:")

	for _, d := range dirs {
		if !d.IsDir() {
			continue
		}

		gatherName := d.Name()

		file, err := os.Open(path + "/gathers/" + gatherName + "/gather.yaml")

		if err != nil {
			fmt.Printf("Can't load gatherer %v", gatherName)
			return err
		}

		fileInfo, err := file.Stat()

		if err != nil {
			fmt.Printf("Can't load gatherer %v", gatherName)
			return err
		}

		data := make([]byte, fileInfo.Size())

		_, err = file.Read(data)

		if err != nil {
			fmt.Println("Failed to read contents of gather.yaml file")
			return err
		}

		var gather GatherInfo

		err = yaml.Unmarshal(data, &gather)

		if err != nil {
			fmt.Println("Failed to parse gather.yaml file!")
			fmt.Printf("Error: %v", err)
			return err
		}

		file.Close()

		fmt.Printf("Name: %v\n Description: %v\n Author: %v\n Version: %v\n Command: %v\n Arguments:%v\n\n",
			gather.Name, gather.Description, gather.Author, gather.Version, gather.Command, gather.Arguments)
	}

	return nil
}

func loadGatherers(path string) error {
	dirs, err := ioutil.ReadDir(path + "/gathers")

	if err != nil {
		fmt.Println("Unable to open gathers folder... Skipping")
		return nil
	}

	for _, d := range dirs {
		if !d.IsDir() {
			continue
		}

		gatherName := d.Name()

		fmt.Printf("Running Gatherer: %v\n", gatherName)

		file, err := os.Open(path + "/gathers/" + gatherName + "/gather.yaml")

		if err != nil {
			fmt.Printf("Failed to load gather %v\n", gatherName)
			return err
		}

		fileInfo, err := file.Stat()

		if err != nil {
			fmt.Println("Failed to get file size of gather.yaml!")
			return err
		}

		data := make([]byte, fileInfo.Size())

		_, err = file.Read(data)

		if err != nil {
			fmt.Println("Failed to read contents of gather.yaml file")
			return err
		}

		var gather GatherInfo

		err = yaml.Unmarshal(data, &gather)

		if err != nil {
			fmt.Println("Failed to parse gather.yaml file!")
			fmt.Printf("Error: %v", err)
			return err
		}

		file.Close()

		curentDir, _ := os.Getwd()
		os.Chdir(path + "/gathers/" + gatherName)

		//Execute gatherer
		cmd := exec.Command(gather.Command, gather.Arguments...)

		out, err := cmd.CombinedOutput()

		if err != nil {
			fmt.Printf("Error running gather!\nError: %v\n", string(out))
			return err
		}

		//Capture variables form output
		vars := getVarsFromStd(string(out))
		msgs := getMessagesFromStd(string(out))

		//set environment variables
		for key, value := range vars {
			fmt.Printf("Gatherer found %v = %v\n", key, value)
			os.Setenv(key, value)
		}

		for _, msg := range msgs {
			fmt.Printf("MSG: %v\n", msg)
		}

		os.Chdir(curentDir)
	}

	return nil
}

func getVarsFromStd(text string) map[string]string {
	returnData := make(map[string]string)

	re := regexp.MustCompile("##SPANR\\[(.*)=(.*)\\]##")

	result := re.FindAllStringSubmatch(text, -1)

	for _, item := range result {
		returnData[item[1]] = item[2]
	}

	return returnData
}

func getMessagesFromStd(text string) []string {
	var returnData []string

	re := regexp.MustCompile("##SPANRMSG\\[(.*)\\]##")

	result := re.FindAllStringSubmatch(text, -1)

	for _, item := range result {
		returnData = append(returnData, item[1])
	}

	return returnData
}

func saveResult(path string, config ConfigInfo) error {
	data, err := yaml.Marshal(config)

	if err != nil {
		fmt.Println("Failed to serialize config info")
		return err
	}

	file, err := os.Create(path)

	if err != nil {
		fmt.Println("Failed to create output file!")
		return err
	}

	_, err = file.Write(data)

	if err != nil {
		fmt.Println("Failed while writing data to output file!")
		return err
	}

	file.Close()

	return nil
}

func loadProperties(path string) error {
	if path == "" {
		return nil
	}

	file, err := os.Open(path)

	if err != nil {
		return err
	}

	fileInfo, err := file.Stat()

	if err != nil {
		fmt.Println("Failed to get properties file size!")
		return err
	}

	data := make([]byte, fileInfo.Size())

	_, err = file.Read(data)

	if err != nil {
		fmt.Println("Failed to read contents of properties file")
		return err
	}

	var results map[string]string

	err = yaml.Unmarshal(data, &results)

	if err != nil {
		fmt.Println("Failed to parse propteries file!")
		fmt.Printf("Error: %v", err)
		return err
	}

	file.Close()

	fmt.Println("Applying properties:")

	for key, value := range results {
		fmt.Printf("Setting %v = %v\n", key, value)
		os.Setenv(key, value)
	}

	return nil
}

func loadRuntimes(path string) error {

	file, err := os.Open(path + "/runtimes.yaml")

	if err != nil {
		fmt.Println("No run times found... Skipping")
		return nil
	}

	fileInfo, err := file.Stat()

	if err != nil {
		fmt.Println("Failed to get runtime.yaml file size!")
		return err
	}

	data := make([]byte, fileInfo.Size())

	if err != nil {
		fmt.Println("Failed to create buffer to store runtime.yaml in!")
		return err
	}

	_, err = file.Read(data)

	if err != nil {
		fmt.Println("Failed to read contents of runtime.yaml")
		return err
	}

	var results []RunTimeInfo

	err = yaml.Unmarshal(data, &results)

	if err != nil {
		fmt.Println("Failed to parse runtime.yaml!")
		fmt.Printf("Error: %v", err)
		return err
	}

	file.Close()

	newPath := os.Getenv("PATH")

	for _, rt := range results {
		fmt.Printf("Adding runtime %v to path\n", rt.Name)
		for _, p := range rt.Path {
			newPath = path + "/" + p + ";" + newPath
		}
	}

	os.Setenv("PATH", newPath)
	return nil

}
