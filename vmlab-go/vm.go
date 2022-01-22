package main

import (
	"path/filepath"
  "os"
  "fmt"
  "errors"
  "github.com/shirou/gopsutil/v3/process"
  "strings"
  "net"
  "time"
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

  socketPath := filepath.Join(root, vm.Name, "control.sock")

  command := fmt.Sprintf("qemu-system-x86_64 -name %s -cpu host -enable-kvm -m %d -smp %d -drive file=%s -vga std -qmp unix:%s,server &", vm.Name, vm.Memory, vm.Cpus, vmHD, socketPath)

  fmt.Printf("%s\n", command)

  execute(command, "")

  time.Sleep(1 * time.Second)

  sock, err := net.Dial("unix", socketPath)

  if err != nil {
    return err
  }

  sock.Close()

  return nil
}

func vmQmpCommand(vm VirtualMachine, command string) (string, error) {

  root, err := getLocalVMLabDir()

  if err != nil {
    return "", err
  }

  socketPath := filepath.Join(root, vm.Name, "control.sock")

  //If vm socket doesn't exist then its not running.
  if !exists(socketPath) {
    return "", nil
  }

  sock, err := net.Dial("unix", socketPath)

  if err != nil {
    return "", err
  }

  defer sock.Close()

  txt, err := readSocket(sock)

  if err != nil {
    return "", err 
  }

  if strings.Index(txt, "QMP") == -1 {
    return "", errors.New("Did not return expected version string")
  }

  err =  writeSocket(sock, "{ \"execute\": \"qmp_capabilities\" }")

  if err != nil {
    return "", err
  }

  txt, err = readSocket(sock)

  if err != nil {
    return "", err 
  }

  if strings.Index(txt, "return") == -1 {
    return "", errors.New("Operation Failed")
  }

  err =  writeSocket(sock, fmt.Sprintf("{ \"execute\": \"%s\" }", command))

  if err != nil {
    return "", err
  }

  txt, err = readSocket(sock)

  if err != nil {
    return "", err 
  }
 
  return txt, nil
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

func vmShutdown(vm VirtualMachine) error {

  _, err := vmQmpCommand(vm, "system_powerdown")

  return err

}

func vmDestroy(vm VirtualMachine) error {
  root, err := getLocalVMLabDir()

  if err != nil {
    return err
  }

  vmDir := filepath.Join(root, vm.Name)

  //If vm doesn't exist the vm isn't provisioned so we don't need to destroy this one.
  if !exists(vmDir) {
    return nil
  }

  err = os.RemoveAll(vmDir)

  return err
}
