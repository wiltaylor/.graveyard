package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/tucnak/climax"
)

func main() {
	clihandler := climax.New("spanr-test")
	clihandler.Brief = "Spanr Test Tool"
	clihandler.Version = "0.1.0"

	runCmd := climax.Command{
		Name: "run",
		Flags: []climax.Flag{
			{
				Name:     "test",
				Short:    "t",
				Usage:    "--test",
				Help:     "Runs a list of tests sperated by ,",
				Variable: true,
			},
			{
				Name:     "output",
				Short:    "o",
				Usage:    "--output",
				Help:     "Path to output report",
				Variable: true,
			},
		},
		Brief: "Runs a test suite",
		Handle: func(ctx climax.Context) int {

			if len(ctx.Args) != 1 {
				fmt.Println("You need to pass in a path to your test suite!")
				os.Exit(5)
			}

			tests, err := loadTests(ctx.Args[0])

			if err != nil {
				fmt.Println("Failed to load test suite! Error: {}", err)
				os.Exit(5)
			}
			absPath, _ := filepath.Abs(ctx.Args[0])
			workingDir := filepath.Dir(absPath)
			oldWorkingDir, _ := os.Getwd()

			os.Chdir(workingDir)

			target := ctx.Variable["test"]

			result := runTests(target, tests)
			os.Chdir(oldWorkingDir)

			outfile := ctx.Variable["output"]
			returnCode := 0

			for _, r := range result {
				if r.Result == TestPassed || r.Result == TestIgnored {
					continue
				}

				returnCode = 4
			}

			if outfile != "" {

				jsonData, err := json.MarshalIndent(result, "", "    ")

				if err != nil {
					fmt.Println("Failed to generate json data!")
					os.Exit(5)
				}

				file, err := os.Create(outfile)

				if err != nil {
					fmt.Printf("Failed to create json file! %v Error: %v", outfile, err)
					os.Exit(5)
				}

				_, err = file.Write(jsonData)

				if err != nil {
					fmt.Printf("Failed to create json file! %v Error: %v", outfile, err)
					os.Exit(5)
				}
			}

			os.Exit(returnCode)

			return 0
		},
	}

	listCmd := climax.Command{
		Name:  "list",
		Brief: "Lists all the tests in a test suite",
		Handle: func(ctx climax.Context) int {

			if len(ctx.Args) != 1 {
				fmt.Println("You need to pass in a path to your test suite!")
				os.Exit(5)
			}

			result, err := loadTests(ctx.Args[0])

			if err != nil {
				fmt.Println("Failed to load test suite! Error: {}", err)
				os.Exit(5)
			}

			for _, v := range result {
				fmt.Printf("[%v]\n", v.Name)
				fmt.Printf("%v\n", v.Description)
				fmt.Printf("Command: %v %v\n", v.Command, v.Arguments)
				fmt.Println("Tests:")

				for _, t := range v.Tests {
					fmt.Printf(" * %v\n", t)
				}

				fmt.Println("")

			}

			os.Exit(0)

			return 0
		},
	}

	clihandler.AddCommand(runCmd)
	clihandler.AddCommand(listCmd)
	clihandler.Run()
}
