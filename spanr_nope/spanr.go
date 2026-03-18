package main

import (
	"fmt"
	"os"

	"github.com/tucnak/climax"
)

func main() {
	clihandler := climax.New("spanr")
	clihandler.Brief = "Spanr Configuration Management Tool"
	clihandler.Version = "0.1.0"

	initCmd := climax.Command{
		Name:  "init",
		Brief: "Creates a configuration management folder structure",
		Handle: func(ctx climax.Context) int {
			result := createFolder(ctx.Args[0])

			if result != 0 {
				os.Exit(result)
			}

			return 0
		},
	}

	listcmd := climax.Command{
		Name:  "ls",
		Brief: "Lists information about a configuration folder",
		Handle: func(ctx climax.Context) int {
			listConfig(ctx.Args[0])
			return 0
		},
	}

	runCmd := climax.Command{
		Name:  "run",
		Brief: "Applies a configuration to system",
		Flags: []climax.Flag{
			{
				Name:     "properties",
				Short:    "p",
				Usage:    "--properties",
				Help:     "Properties to be passed into configuration",
				Variable: true,
			},
			{
				Name:     "test",
				Short:    "t",
				Usage:    "--test",
				Help:     "Runs tests only, does not configure system",
				Variable: false,
			},
			{
				Name:     "config",
				Short:    "c",
				Usage:    "--config",
				Help:     "Specify an alternative config file",
				Variable: true,
			},
			{
				Name:     "output",
				Short:    "o",
				Usage:    "--output",
				Help:     "Specify path to config result",
				Variable: true,
			},
		},
		Handle: func(ctx climax.Context) int {
			outFile := ctx.Variable["output"]
			test := ctx.NonVariable["test"]

			result, cfg := runConfig(ctx.Args[0], ctx.Variable["properties"], test, ctx.Variable["config"])

			if outFile != "" {
				saveResult(outFile, cfg)
			}

			fmt.Printf("Overall State: %v\n", printCFG(result))

			if result == CFGRebootRequired {
				os.Exit(3010)
			} else if result == CFGConfigured {
				os.Exit(0)
			} else {
				os.Exit(5)
			}

			return 0
		},
	}

	clihandler.AddCommand(initCmd)
	clihandler.AddCommand(runCmd)
	clihandler.AddCommand(listcmd)
	clihandler.Run()
}
