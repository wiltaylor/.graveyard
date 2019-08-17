package main

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"time"
)

//Test state constants
const (
	TestError   = iota //Test command failed to run
	TestFailed  = iota //Test command ran but reported fail or nothing
	TestIgnored = iota //Test reported ignored
	TestWarning = iota //Test reported warning
	TestPassed  = iota //Test reported it passed
)

//TestResultInfo - Contains the results of a test run
type TestResultInfo struct {
	SetName  string //Name of set test was from
	TestName string //Name of test that ran
	Result   int    //Result of tests see test state constants
}

//Runs tests passed to it. If a target string is prestent it will only
//run listed tests.
func runTests(target string, tests []TestSet) []TestResultInfo {
	targetList := strings.Split(target, ",")

	var returnData []TestResultInfo

	for _, set := range tests {
		for _, test := range set.Tests {
			if target != "" {
				found := false

				for _, i := range targetList {
					if test == i {
						found = true
						break
					}
				}

				if !found {
					continue
				}
			}

			fmt.Printf("Executing [%v]:%v", set.Name, test)

			output, err := executeTest(set.Command, set.Arguments, test, set.TimeOut)

			result := TestResultInfo{
				SetName:  set.Name,
				TestName: test,
				Result:   TestFailed,
			}

			if err != nil {
				result.Result = TestError
				returnData = append(returnData, result)
				fmt.Printf(" - ERROR: %v\n", err)
				fmt.Println(output)
				continue
			}

			index := strings.Index(output, "##FAIL##")
			if index != -1 {
				returnData = append(returnData, result)
				fmt.Println(" - FAILED")
				fmt.Println(output)
				continue
			}

			index = strings.Index(output, "##IGNORE##")
			if index != -1 {
				result.Result = TestIgnored
				returnData = append(returnData, result)
				fmt.Println(" - IGNORE")
				continue
			}

			index = strings.Index(output, "##WARN##")
			if index != -1 {
				result.Result = TestWarning
				returnData = append(returnData, result)
				fmt.Println(" - WARN")
				continue
			}

			index = strings.Index(output, "##PASS##")
			if index != -1 {
				result.Result = TestPassed
				returnData = append(returnData, result)
				fmt.Println(" - PASS")
				continue
			}

			fmt.Println(" - FAILED")
			fmt.Println(output)
			returnData = append(returnData, result)
		}
	}

	return returnData
}

//Executes a test passed to it and returns the stdout and stderr combined.
func executeTest(command string, args []string, test string, timeout int) (string, error) {
	pwd, _ := os.Getwd()
	wrkargs := make([]string, len(args))
	copy(wrkargs, args)

	for i := range wrkargs {
		wrkargs[i] = strings.Replace(wrkargs[i], "{TESTNAME}", test, 100)
		wrkargs[i] = strings.Replace(wrkargs[i], "{PWD}", pwd, 100)
	}

	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(timeout)*time.Second)

	cmd := exec.CommandContext(ctx, command, wrkargs...)
	out, err := cmd.CombinedOutput()
	cancel()
	if err != nil {
		return string(out), err
	}

	return string(out), nil
}

func printResult(result TestResultInfo) {
	switch result.Result {
	case TestError:
		fmt.Printf("[%v]:%v - ERROR\n", result.SetName, result.TestName)
		break
	case TestFailed:
		fmt.Printf("[%v]:%v - FAILED\n", result.SetName, result.TestName)
		break
	case TestIgnored:
		fmt.Printf("[%v]:%v - IGNORED\n", result.SetName, result.TestName)
		break
	case TestWarning:
		fmt.Printf("[%v]:%v - WARNING\n", result.SetName, result.TestName)
		break
	case TestPassed:
		fmt.Printf("[%v]:%v - PASSED\n", result.SetName, result.TestName)
		break
	}
}
