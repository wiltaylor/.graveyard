package main

import (
	"path/filepath"
  "os"
  "fmt"
  "errors"
  "github.com/shirou/gopsutil/v3/process"
  "strings"
)

func getTemplateHd(name string) (string, error) {

  root, err := getVMLabRoot()

  if err != nil {
    return "", err
  }

  template := filepath.Join(root, "qemu", name, "main.qcow")

  if !exists(template) {
    return "", errors.New("Template doesn't exists!")
  }

  return template, nil

}

func provisionVM(vm VirtualMachine) error {

  root, err := getLocalVMLabDir()

  if err != nil {
    return nil
  }

  vmPath := filepath.Join(root, vm.Name)

  if !exists(vmPath) {
    err = os.Mkdir(vmPath, 0755)

    if err != nil {
      return err
    }
  }

  hdPath := filepath.Join(vmPath, "main.qcow")

  if !exists(hdPath) {
    baseHd, err := getTemplateHd(vm.Template)

    if err != nil {
      return err
    }
    
    command := fmt.Sprintf("qemu-img create -f qcow2 -F qcow2 -b %s %s", baseHd, hdPath)
    err = execute(command, "")
  }

  return nil
}

func vmStart(vm VirtualMachine) error {

  root, err := getLocalVMLabDir()

  if err != nil {
    return err
  }

  vmHD := filepath.Join(root, vm.Name, "main.qcow")

  if !exists(vmHD) {
    return errors.New("VM HD is missing.")
  }

  command := fmt.Sprintf("qemu-system-x86_64 -name %s -cpu host -enable-kvm -m %d -smp %d -drive file=%s -vga std &", vm.Name, vm.Memory, vm.Cpus, vmHD)

  fmt.Printf("%s\n", command)

  execute(command, "")

  return nil
}

func vmStop(vm VirtualMachine) error {
  root, err := getLocalVMLabDir()

  if err != nil {
    return err
  }

  vmHD := filepath.Join(root, vm.Name, "main.qcow")

  //If vm doesn't exist the vm isn't provisioned so we don't need to stop this one.
  if !exists(vmHD) {
    return nil
  }

  processes, err := process.Processes()
  if err != nil {
    return err 
  }

  for _, p := range processes {
    cmd, _ := p.Cmdline()
    if strings.Index(cmd, vmHD) != -1 {
      p.Kill()
    }
  }

  return nil
}
