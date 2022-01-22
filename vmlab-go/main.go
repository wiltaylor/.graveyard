package main

import (
  "fmt"
  "errors"
  "os"
	"path/filepath"
)

const VERSION = "0.1.0"

func getVMLabRoot() (string, error) {
  labRoot := os.Getenv("VMLABROOT")

  if labRoot == "" {
    labRoot = filepath.Join(os.Getenv("HOME"), ".local", "share", "vmlab")
  }

  if !exists(labRoot) {
    err := os.Mkdir(labRoot, 0755)

    if err != nil {
      return "", err
    }
  }

  return labRoot, nil
}

func getLocalVMLabDir() (string, error) { 
	pwd, err := os.Getwd()

  if err != nil {
    return "", err
  }

  rootPath := filepath.Join(pwd, ".vmlab")

  if !exists(rootPath) {
    err = os.Mkdir(rootPath, 0755)

    if err != nil {
      return "", err
    }
  }

  return rootPath, nil
}

func getVMLabFilePath() (string, error) {
  pwd, err := os.Getwd()

  if err != nil {
    return "", err
  }

  vmlabPath := filepath.Join(pwd, "vmlab.yaml")

  if !exists(vmlabPath) {
    return "", errors.New("VMLab file doesn't exist!")
  }

  return vmlabPath, nil
}

func getCommandAndArgs(args []string) (string, []string, error) {
  if len(args) == 0 {
    return "", make([]string, 0), errors.New("Expected there to be at least 1 argument")
  }

  command := args[0]
  commandargs := make([]string, 0)

  if len(args) > 2 {
    commandargs = args[1:]
  }

  return command, commandargs, nil
}

func initCommand() error {
  blankLab := vmlabFile{}
  blankLab.Title = "Default VMLab File"
  blankLab.Description = "This is a vmlab file created by vmlab init. You should update these properties to describe your lab environment"
  blankLab.Author = os.Getenv("USERNAME")
  blankLab.VirtualMachines = make([]VirtualMachine, 1)
  blankLab.VirtualMachines[0].Name = "myVM"
  blankLab.VirtualMachines[0].Template = "ubuntu-21.10"
  blankLab.VirtualMachines[0].Cpus = 2
  blankLab.VirtualMachines[0].Memory = 2048

	pwd, err := os.Getwd()

  if err != nil {
    return err
  }

  vmlabPath := filepath.Join(pwd, "vmlab.yaml")

  if exists(vmlabPath) {
    return errors.New("vmlab.yaml file already exists!")
  }

  err = writeLabFile(vmlabPath, blankLab)

  return err
}

func infoCommand() error{
  vmlabPath, err := getVMLabFilePath()

  if err != nil {
    return errors.New("This directory doesn't have a vmlab.yaml file in it!")
  }

  lab, err := loadLabFile(vmlabPath)

  fmt.Printf("Lab Info:\n")
  fmt.Printf("Title: %s\n", lab.Title)
  fmt.Printf("Description: %s\n", lab.Description)
  fmt.Printf("Author: %s\n", lab.Author)
  fmt.Printf("Virtual Machines:\n")

  for _, vm := range lab.VirtualMachines {
    fmt.Printf("  Name: %s\n", vm.Name)
    fmt.Printf("  Status: Unprovisoned\n")
    fmt.Printf("  Template: %s\n", vm.Template)
    fmt.Printf("  CPUs: %d\n", vm.Cpus)
    fmt.Printf("  Memory: %d MiB\n", vm.Memory)
    fmt.Printf("\n")
  }

  return nil
}

func upCommand() error {
  vmlabPath, err := getVMLabFilePath()

  if err != nil {
    return errors.New("This directory doesn't have a vmlab.yaml file in it!")
  }

  vmlabFile, err := loadLabFile(vmlabPath)

  if err != nil {
    return err
  }

  for _, vm := range vmlabFile.VirtualMachines {
    err = provisionVM(vm)

    if err != nil {
      return err
    }

    err = vmStart(vm)

    if err != nil {
      return err
    }
  }

  return nil
}

func stopCommand() error {
  vmlabPath, err := getVMLabFilePath()

  if err != nil {
    return errors.New("This directory doesn't have a vmlab.yaml file in it!")
  }

  vmlabFile, err := loadLabFile(vmlabPath)

  if err != nil {
    return err
  }

  for _, vm := range vmlabFile.VirtualMachines {

    //err = vmStop(vm)
    err = vmShutdown(vm)

    if err != nil {
      return err
    }
  }

  return nil
}

func destroyCommand() error {
  vmlabPath, err := getVMLabFilePath()

  if err != nil {
    return errors.New("This directory doesn't have a vmlab.yaml file in it!")
  }

  vmlabFile, err := loadLabFile(vmlabPath)

  if err != nil {
    return err
  }

  for _, vm := range vmlabFile.VirtualMachines {

    err = vmStop(vm)

    if err != nil {
      return err
    }

    err = vmDestroy(vm)

    if err != nil {
      return err
    }
  }

  return nil
}

func printIfErr(err error) {
  if err != nil {
    fmt.Printf("Error: %s\n", err)
  }
}

func main() {
  if len(os.Args) == 1 {
    usage()
    return
  }

  command, args, err := getCommandAndArgs(os.Args[1:])

  if err != nil {
    usage()
    return
  }

  switch(command) {
    case "version":
      version(args)
      break
    case "info":
      printIfErr(infoCommand())
      break
    case "up":
      printIfErr(upCommand())
      break
    case "stop":
      printIfErr(stopCommand())
      break
    case "destroy":
      printIfErr(destroyCommand())
      break
    case "init":
      printIfErr(initCommand())
      break
    default:
      usage()
      break
  }
}

func version(args []string){
  fmt.Printf("VMLAB Version: %s\n", VERSION)
}

func usage(){
  fmt.Println("vmlab usage:")
  fmt.Println("vmlab {command} [options]")
  fmt.Println()
  fmt.Println("Commands:")
  fmt.Println("Up - Provisions VMs if they are not already and powers them on")
  fmt.Println("Stop - Stops running VMs")
  fmt.Println("Destroy - Removes all lab vm data from disk")
  fmt.Println("init - Creates a new vmlab.yaml file in the current directory")
  fmt.Println("info - Prints info of the lab.")
  fmt.Println("version - Prints vmlab version information")
}
