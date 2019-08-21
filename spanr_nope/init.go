package main

import (
	"fmt"
	"os"
)

func createFolder(path string) int {
	targetDir := path

	if targetDir == "" {
		targetDir, _ = os.Getwd()
	}

	err := os.MkdirAll(targetDir, 0755)

	if err != nil {
		fmt.Println("Unable to create target directory!")
		return 5
	}

	//Create resources
	err = os.MkdirAll(targetDir+"/resources", 0755)

	if err != nil {
		fmt.Println("Unable to create resources directory!")
		fmt.Printf("%v", err)
		return 5
	}

	//Create gathers
	err = os.MkdirAll(targetDir+"/gathers", 0755)

	if err != nil {
		fmt.Println("Unable to create gathers directory!")
		return 5
	}

	//Create runtimes
	err = os.MkdirAll(targetDir+"/runtimes", 0755)

	if err != nil {
		fmt.Println("Unable to create runtimes directory!")
		return 5
	}

	//runtimes.yaml
	file, err := os.Create(targetDir + "/runtimes.yaml")

	if err != nil {
		fmt.Println("Unable to create runtime.yaml file!")
		return 5
	}

	_, err = file.WriteString(`
# Use this file to add run times to the path from the runtimes directory.
# Add them as a list in the following format:
# eg:
# - name: python
#   path: ['python/bin', 'python/lib']
# - name: foo
#   path: ['foo']
#
# All paths above are relative to the runtimes directory.`)

	if err != nil {
		fmt.Println("Failed to write to runtime.yaml file!")
		return 5
	}

	err = file.Close()

	if err != nil {
		fmt.Println("Failed to close runtimes.yaml file!")
		return 5
	}

	//config.yaml
	file, err = os.Create(targetDir + "/config.yaml")

	if err != nil {
		fmt.Println("Failed to create config.yaml!")
		return 5
	}

	_, err = file.WriteString(`
# Fill out the file metadata here
name: "Name of config here"
author: "Your name here"
description: "Your script description here"
version: "0.1.0"
Items:
# Fill out configuration items here
- Name: MyConfig #Name must be unique for each item
Resource: MyResource #Name of resource that this item uses.
Condition: VarName1 #Variable that must be true to run this (put ! at front for false)
PreReq: [ "MyConfig2", "MyConfig3" ] # List of other configuration items that must be run before this one
Options: #Options for the resource this action applies to.
  Op1: "5" #You can put variables in options by putting
  Op2: "123" #$VARNAME or ${VARNAME}`)

	if err != nil {
		fmt.Println("Failed to write to config.yaml!")
		return 5
	}

	err = file.Close()

	if err != nil {
		fmt.Println("Failed to close config.yaml")
		return 5
	}

	//properties.yaml
	file, err = os.Create(targetDir + "/properties.yaml")

	if err != nil {
		fmt.Println("Failed to create properties.yaml!")
		return 5
	}

	_, err = file.WriteString(`
# Put a list of properties that can be passed into a configuration
# Have to be key value pairs and will end up as environment variables
MyProperty: 'MyValue'
MyOtherProperty: 'MyOtherValue'
`)

	if err != nil {
		fmt.Println("Failed to write to properties.yaml")
		return 5
	}

	err = file.Close()

	if err != nil {
		fmt.Println("Failed to close properties.yaml")
		return 5
	}

	fmt.Println("Created empty configuration layout.")

	return 0

}
