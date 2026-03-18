package main

import (
	"os"

	"gopkg.in/yaml.v2"
)

//TestSet - A definition of a group of tests included in a file.
type TestSet struct {
	Name            string   //Name of test set
	Description     string   //Description of group of tests in current file.
	Command         string   //Command used to run tests
	Arguments       []string //Arguments passed to run tests. Put token {TESTNAME} to have test name passed into the arguments
	Tests           []string //Names of tests included in test set.
	TimeOut         int      //Length of time in seconds that tests can run before timing out.
	ContinueOnError bool     //Continue on error
}

func loadTests(path string) ([]TestSet, error) {
	var result []TestSet

	file, err := os.Open(path)

	if err != nil {
		return []TestSet{}, err
	}

	fileInfo, err := file.Stat()

	if err != nil {
		return []TestSet{}, err
	}

	data := make([]byte, fileInfo.Size())
	file.Read(data)

	err = yaml.Unmarshal(data, &result)

	if err != nil {
		return []TestSet{}, err
	}

	return result, nil
}
