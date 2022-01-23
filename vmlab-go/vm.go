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
  "regexp"
	"encoding/base64"
  "math/rand"
)

func getTemplateHd(template TemplateFile) (string, error) {

  root, err := getVMLabRoot()

  if err != nil {
    return "", err
  }

  fmt.Printf("%v\n", template)

  templatePath := filepath.Join(root, "qemu", template.Name, "main.qcow")

  if !exists(templatePath) {
    return "", errors.New("Template doesn't exists!")
  }

  return templatePath, nil

}

func provisionVM(vm VirtualMachine, template TemplateFile) error {

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
    baseHd, err := getTemplateHd(template)

    if err != nil {
      return err
    }
    
    command := fmt.Sprintf("qemu-img create -f qcow2 -F qcow2 -b %s %s", baseHd, hdPath)
    err = execute(command, "")
  }

  return nil
}

func vmStart(vm VirtualMachine, template TemplateFile) error {

  root, err := getLocalVMLabDir()

  if err != nil {
    return err
  }

  vmHD := filepath.Join(root, vm.Name, "main.qcow")

  if !exists(vmHD) {
    return errors.New("VM HD is missing.")
  }

  socketPath := filepath.Join(root, vm.Name, "control.sock")
  guestSocketPath := filepath.Join(root, vm.Name, "guest.sock")

  shareCommand := ""
  networkCommand := ""

  for _, share := range vm.SharedFolders {
    shareCommand += fmt.Sprintf("-fsdev local,security_model=mapped,id=fsdev-%s,multidevs=remap,path=%s -device virtio-9p-pci,id=%s,fsdev=fsdev-%s,mount_tag=%s ",
      share.Name, share.Host, share.Name, share.Name, share.Name)    
  }

  for _, net := range vm.Networks {
    
    if net.Type == "public" {
      networkCommand += "-nic user "
    }

    if net.Type == "private" {
      networkCommand += fmt.Sprintf("-device e1000,netdev=nd-%s -netdev socket,id=nd-%s,mcast=230.0.0.1:%d ", net.Name, net.Name, net.Port)
    }
  }

  if networkCommand == "" {
    networkCommand = "-nic none"
  }

  OSCommand := ""

  if template.GuestOS == "windows" {
    OSCommand = "-usb -device usb-tablet" 
  }

  clientNo := rand.Intn(10000)


  command := fmt.Sprintf("qemu-system-x86_64 -name %s -cpu host -enable-kvm -m %d -smp %d -drive node-name=drive0,file=%s -display sdl -vga virtio -qmp unix:%s,server=on,wait=off -chardev socket,path=%s,server=on,wait=off,id=qga0 -device virtio-serial -device virtserialport,chardev=qga0,name=org.qemu.guest_agent.%d %s %s %s &",
    vm.Name, vm.Memory, vm.Cpus, vmHD, socketPath, guestSocketPath, clientNo, shareCommand, networkCommand, OSCommand)

  fmt.Printf("%s\n", command)

  execute(command, "")

  //sleeping for 5 to work around a bug with windows where if you don't wait longer it hangs
  time.Sleep(5 * time.Second)

  if shareCommand != "" {
    for {
      status := vmGetStatus(vm)

      if status == "Unprovisioned" {
        return errors.New("VM is not provisioned while waiting for it to be ready to provision shared drives")
      }

      if status == "running" {
        for _, share := range vm.SharedFolders {
          err = vmExecute(vm, fmt.Sprintf("mkdir -p %s", share.Guest))
          if err != nil {
            return err
          }

          err = vmExecute(vm, fmt.Sprintf("mount -t 9p -o trans=virtio %s %s -oversion=9p2000.L,posixacl,msize=5000000,cache=mmap", share.Name, share.Guest))
          if err != nil {
            return err
          }
        }

        break
      }
    }
  }

  return nil
}

func vmGACommand(vm VirtualMachine, json string) (string, error) {
  root, err := getLocalVMLabDir()

  if err != nil {
    return "", err
  }

  socketPath := filepath.Join(root, vm.Name, "guest.sock")

  //If vm socket doesn't exist then its not running.
  if !exists(socketPath) {
    return "", errors.New("Can't find guest.sock file for this vm!")
  }

  sock, err := net.Dial("unix", socketPath)

  if err != nil {
    return "", err
  }

  defer sock.Close()

  err =  writeSocket(sock, json)

  txt, err := readSocket(sock)

  if err != nil {
    return "", err 
  }

  return txt, nil
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

func vmRestart(vm VirtualMachine) error {
  _, err := vmQmpCommand(vm, "system_reset")

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

func vmGetStatus(vm VirtualMachine) string {

  root, err := getLocalVMLabDir()

  if err != nil {
    return "Error"
  }


  vmHD := filepath.Join(root, vm.Name, "main.qcow")
  socketPath := filepath.Join(root, vm.Name, "control.sock")

  if !exists(vmHD) {
    return "Unprovisioned"
  }

  if !exists(socketPath) {
    return "Off"
  }

  txt, err := vmQmpCommand(vm, "query-status")
  
  if err != nil {
    return "Error"
  }

  re := regexp.MustCompile(`"status":."(.*)"[,}]`)

  for _, match := range re.FindAllStringSubmatch(txt, -1) {
    for i, group := range match {
      if i == 0 {
        continue
      }
      return group
    } 
  }

  return "Unknown"
}

func vmExecute(vm VirtualMachine, command string) error {
  result, err := vmGACommand(vm, fmt.Sprintf(`{ "execute": "guest-exec", "arguments": { "path": "/bin/sh", "arg": [ "-c", "%s" ], "capture-output": true }}`, command))

  if err != nil {
    return err
  }

  pid := regexFirstGroup(`"pid": ([0-9]{1,8})`, result)

  if pid == "" {
    fmt.Printf("%s\n", result)
    return errors.New("Was unable to run command!")
  }

  for {

    result, err = vmGACommand(vm, fmt.Sprintf(`{ "execute": "guest-exec-status", "arguments": { "pid": %s }}`, pid))

    if err != nil {
      return err
    }

    exited := regexFirstGroup(`"exited":.(true)`, result)
    txt := regexFirstGroup(`"out-data": "([0-9a-zA-Z=]+)"[,}]`, result)

    decodedTxt, err := base64.StdEncoding.DecodeString(txt)

    if err != nil {
      return err
    }

    fmt.Printf("%s", decodedTxt)

    if exited == "true" {
      break
    }
  }

  return nil
}

func vmCreateSnapshot(vm VirtualMachine, name string) error {
  root, err := getLocalVMLabDir()

  if err != nil {
    return err
  }

  vmHD := filepath.Join(root, vm.Name, "main.qcow")

  //If vm doesn't exist the vm isn't provisioned so we don't need to stop this one.
  if !exists(vmHD) {
    return nil
  }

  command := fmt.Sprintf("qemu-img snapshot -q -c \"%s\" %s", name, vmHD)

  err = execute(command, "")

  return err
}

func vmListSnapshot(vm VirtualMachine) error {
  root, err := getLocalVMLabDir()

  if err != nil {
    return err
  }

  vmHD := filepath.Join(root, vm.Name, "main.qcow")

  //If vm doesn't exist the vm isn't provisioned so we don't need to stop this one.
  if !exists(vmHD) {
    return nil
  }

  command := fmt.Sprintf("qemu-img info %s", vmHD)

  err = execute(command, "")

  return err
}

func vmRemoveSnapshot(vm VirtualMachine, name string) error {
  root, err := getLocalVMLabDir()

  if err != nil {
    return err
  }

  vmHD := filepath.Join(root, vm.Name, "main.qcow")

  //If vm doesn't exist the vm isn't provisioned so we don't need to stop this one.
  if !exists(vmHD) {
    return nil
  }

  command := fmt.Sprintf("qemu-img snapshot -q -d \"%s\" %s", name, vmHD)

  err = execute(command, "")

  return err
}

func vmRevertToSnapshot(vm VirtualMachine, name string) error {
  root, err := getLocalVMLabDir()

  if err != nil {
    return err
  }

  vmHD := filepath.Join(root, vm.Name, "main.qcow")

  //If vm doesn't exist the vm isn't provisioned so we don't need to stop this one.
  if !exists(vmHD) {
    return nil
  }

  command := fmt.Sprintf("qemu-img snapshot -q -a \"%s\" %s", name, vmHD)

  err = execute(command, "")

  return err
}
