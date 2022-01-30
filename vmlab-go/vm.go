package main

import (
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io/ioutil"
	"net"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/shirou/gopsutil/process"
)

type QemuVMEvent struct {
	name string
	data map[string]interface{}
}

type QemuVMResponse struct {
	error bool
	message string
	value interface{}
}

type QemuVMSocket struct {
	socket net.Conn
	events chan QemuVMEvent
  responses chan QemuVMResponse
	closeSock bool
}

type QemuVM struct {
	SocketConnected bool
  ControlSocket QemuVMSocket
	GuestAgentSocket QemuVMSocket
	GraphObj VirtualMachine
	TemplateObj TemplateFile
	HDPath string
	VMPath string
}

type VMSTATUS int

const (
	VMSTATUS_UNPROVISIONED VMSTATUS = iota
	VMSTATUS_OFF
	VMSTATUS_RUNNING
	VMSTATUS_PAUSED
	VMSTATUS_ERROR
	VMSTATUS_UNKNOWN
)

func CreateQemuVMSocket(sockPath string) (QemuVMSocket, error) {
	if !exists(sockPath) {
		return QemuVMSocket{}, errors.New("Socket doesn't exist!")
	}

	sock, err := net.Dial("unix", sockPath)

	if err != nil {
		return QemuVMSocket{}, err
	}

	return QemuVMSocket{
		socket: sock,
		events: make(chan QemuVMEvent, 100),
		responses: make(chan QemuVMResponse, 100),
		closeSock: false,
	}, nil
}

func (target* QemuVM)OpenSockets() error {

	if target.SocketConnected {
		return nil
	}
	
	controlSock, err := CreateQemuVMSocket(filepath.Join(target.VMPath, "control.sock"))

	if err != nil {
		return err
	}

	target.ControlSocket = controlSock
	target.ControlSocket.SendCommand("qmp_capabilities", make(map[string]interface{}))
	
	go controlSock.processSockRead()

	response := target.ControlSocket.ReadResponse()

	if response.error {
		return errors.New("qmp_capabilities responded with an error")
	}

	GASock, err := CreateQemuVMSocket(filepath.Join(target.VMPath, "guest.sock"))
	if err != nil {
		return err
	}

	target.GuestAgentSocket = GASock
	go GASock.processSockRead()

	target.SocketConnected = true

	return nil
	
}

func (target* QemuVM)CloseSockets() error {
	if target.SocketConnected == false {
		return nil
	}

	target.GuestAgentSocket.Close()
	target.ControlSocket.Close()
	target.SocketConnected = false

	return nil
}

func CreateQemuVM(vm* VirtualMachine, template* TemplateFile) (QemuVM, error) {
	root, err := getLocalVMLabDir()

	if err != nil {
		return QemuVM{}, err
	}

	return QemuVM{
		SocketConnected: false,
		GraphObj: *vm,
		TemplateObj: *template,
		HDPath: filepath.Join(root, vm.Name, "main.qcow"),
		VMPath: filepath.Join(root, vm.Name),
	}, nil
}

func (target* QemuVM) StatusString() string {
	switch(target.Status()){
    case VMSTATUS_OFF:
		  return "Off"
		case VMSTATUS_ERROR:
		  return "Error"
		case VMSTATUS_PAUSED:
		  return "Pause"
		case VMSTATUS_RUNNING:
		  return "Running"
	  case VMSTATUS_UNPROVISIONED:
		  return "Unprovisioned"
		default:
		  return "Unknown"
	}
}

func (target* QemuVM) Status() VMSTATUS {

	if !exists(target.HDPath) {
	  return VMSTATUS_UNPROVISIONED	
	}


	if !exists(filepath.Join(target.VMPath, "control.sock")) {
		return VMSTATUS_OFF
	}

	err := target.OpenSockets()

	if err != nil {
		return VMSTATUS_ERROR
	}

	err = target.ControlSocket.SendCommand("query-status", make(map[string]interface{}))

	if err != nil {
		return VMSTATUS_ERROR
	}

	result := target.ControlSocket.ReadResponse() //Eat empty response that comes first.
	result = target.ControlSocket.ReadResponse() 

	if result.error {
		return VMSTATUS_ERROR
	}

	status := result.value.(map[string]interface{})["status"].(string)

	if status == "running" {
		return VMSTATUS_RUNNING
	}

	if status == "paused" {
		return VMSTATUS_PAUSED
	}	
	
	return VMSTATUS_UNKNOWN
}

func (target* QemuVMSocket)Close() {
	target.closeSock = true
	target.socket.Close()
}

func splitLines(txt string) ([]string, string) {
	completed := make([]string, 0)

	start := 0

	for i := 0; i < len(txt); i++ {
		if txt[i] == '\n' {
			completed = append(completed, txt[start:i])
			start = i + 1
		}
	}

	result := txt[start:]
	return completed, result
}

func (target* QemuVMSocket)processSockRead() {
	buffer := make([]byte, 1024)
	text := ""

	fmt.Println("Opened Socket handler")

	for {
		if target.closeSock {
			break
		}

	  n, err := target.socket.Read(buffer)

		if err != nil {
			target.closeSock = true
		  return
		}

		text += string(buffer[0:n])

		lines, text := splitLines(text)

		for _, line := range lines {
			var jsonData map[string]interface{}
			
			err = json.Unmarshal([]byte(line), &jsonData)

			if err != nil {
				target.responses <- QemuVMResponse{ error: true, message: err.Error() }
				continue
			}

			eData, ok := jsonData["QMP"]
			if ok {
				//eat response 
				continue
			}
			
			eData, ok = jsonData["error"]
			if ok {
				errorMessage := eData.(map[string]interface{})["desc"].(string)
				target.responses <- QemuVMResponse { error: true, message: errorMessage }
				continue
			}

			eData, ok = jsonData["event"]
			if ok {
				target.events <- QemuVMEvent { name: eData.(string), data: jsonData}
				continue
			}

			eData, ok = jsonData["return"]
			if ok {
				target.responses <- QemuVMResponse { error: false, value: eData.(map[string]interface{})}
				continue
			}
		}		

		idx := -1

		for i := 0; i < len(text); i++ {
		  if text[i] == '\n' {
			  idx = i
				break
			}
		}

		if idx == -1 {
			continue
		}
	}
}

func (target* QemuVMSocket)WaitingEvents() bool {
	if len(target.events) > 0 {
		return true
	} else {
		return false
	}
}

func (target* QemuVMSocket)WaitingResponses() bool {
	if len(target.responses) > 0 {
		return true
	} else {
		return false
	}
}

type QemuCommandBundle struct {
	Execute string `json:"execute"`
	Arguments map[string]interface{}`json:"arguments"`
}

func (target QemuVMSocket)ReadEvent() QemuVMEvent {
	return <- target.events
}

func (target QemuVMSocket)ReadResponse() QemuVMResponse {
	return <- target.responses
}

func (target* QemuVMSocket)SendCommand(command string, args map[string]interface{}) error {
	cmd := QemuCommandBundle{
		Execute: command,
		Arguments: args,
	}

	cmdTxt, err := json.Marshal(cmd)

	if err != nil {
		return err
	}

	_ , err = target.socket.Write(cmdTxt)

	if err != nil {
		return err
	}

	return nil	
}






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
	portCommand := ""

	CPUCommand := "-cpu host -enable-kvm "

	if template.GuestOS == "windows" {
		CPUCommand = "-cpu host,kvm=on,+hypervisor,+invtsc,l3-cache=on,migratable=no,hv_frequencies,kvm_pv_unhalt,hv_reenlightenment,hv_relaxed,hv_spinlocks=8191,hv_stimer,hv_synic,hv_time,hv_vapic,hv_vendor_id=1234567890ab,hv_vpindex -enable-kvm "
	}

	for _, port := range vm.Ports {
		if portCommand != "" {
			portCommand += ","
		}
		portCommand += fmt.Sprintf("hostfwd=%s::%d-:%d", port.Protocol, port.Host, port.Guest)
	}

	for _, share := range vm.SharedFolders {

		if template.GuestOS == "linux" {
			shareCommand += fmt.Sprintf("-fsdev local,security_model=mapped,id=fsdev-%s,multidevs=remap,path=%s -device virtio-9p-pci,id=%s,fsdev=fsdev-%s,mount_tag=%s ",
				share.Name, share.Host, share.Name, share.Name, share.Name)
		}
	}

	for _, net := range vm.Networks {

		if net.Type == "public" {
			netcmd := "-nic user"

			if portCommand != "" {
				netcmd += "," + portCommand
			}

			networkCommand += netcmd + " "
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
		OSCommand = "-usb -device usb-tablet -machine q35,kernel_irqchip=on "
	}

	command := fmt.Sprintf("qemu-system-x86_64 -name %s %s -m %d -smp %d -drive node-name=drive0,file=%s -display gtk,gl=on -vga virtio -qmp unix:%s,server=on,wait=off -chardev socket,path=%s,server=on,wait=off,id=qga0 -device virtio-serial -device virtserialport,chardev=qga0,name=org.qemu.guest_agent.0 %s %s %s",
		vm.Name, CPUCommand, vm.Memory, vm.Cpus, vmHD, socketPath, guestSocketPath, shareCommand, networkCommand, OSCommand)

	command = strings.Trim(command, " ") + " &"
	fmt.Printf("%s\n", command)

	execute(command, "")

	//sleeping for 5 to work around a bug with windows where if you don't wait longer it hangs
	time.Sleep(1 * time.Second)

	if shareCommand != "" {
		for {
			status := vmGetStatus(vm)

			if status == "Unprovisioned" {
				return errors.New("VM is not provisioned while waiting for it to be ready to provision shared drives")
			}

			if status == "running" {
				for _, share := range vm.SharedFolders {
					if template.GuestOS == "linux" {
						err = vmExecute(vm, template, fmt.Sprintf("mkdir -p %s", share.Guest))
						if err != nil {
							return err
						}

						err = vmExecute(vm, template, fmt.Sprintf("mount -t 9p -o trans=virtio %s %s -oversion=9p2000.L,posixacl,msize=5000000,cache=mmap",
							share.Name, share.Guest))
						if err != nil {
							return err
						}
					}

					if template.GuestOS == "windows" {

					}
				}

				return nil
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

	err = writeSocket(sock, json)

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

	err = writeSocket(sock, "{ \"execute\": \"qmp_capabilities\" }")

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

	err = writeSocket(sock, fmt.Sprintf("{ \"execute\": \"%s\" }", command))

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

func vmExecute(vm VirtualMachine, template TemplateFile, command string) error {
	outCommandPattern := `{ "execute": "guest-exec", "arguments": { "path": "/bin/sh", "arg": [ "-c", "%s" ], "capture-output": true }}`

	if template.GuestOS == "windows" {
		outCommandPattern = `{ "execute": "guest-exec", "arguments": { "path": "c:\\windows\\system32\\cmd.exe", "arg": [ "/c", "%s" ], "capture-output": true }}`
	}

	result, err := vmGACommand(vm, fmt.Sprintf(outCommandPattern, command))

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

func vmCopyFileToGuest(vm VirtualMachine, hostPath string, guestPath string) error {
	outCommandPattern := `{ "execute": "guest-file-open", "arguments": { "path": "%s", "mode": "w" }}`

	result, err := vmGACommand(vm, fmt.Sprintf(outCommandPattern, guestPath))

	if err != nil {
		return err
	}

	guestFileId := regexFirstGroup(`"return": ([0-9]{1,8})`, result)

	if guestFileId == "" {
		fmt.Printf("%s\n", result)
		return errors.New("Was unable to open file on guest!")
	}

	hostFile, err := os.Open(hostPath)

	if err != nil {
		return err
	}

	data, err := ioutil.ReadAll(hostFile)
	index := 0

	if err != nil {
		return err
	}

	for{
		remaining := len(data) - index

		if remaining <= 0 {
			break
		}

		if remaining > 1024 {
			remaining = 1024
		}
		
		curBuffer := data[index:index + remaining]
		base64Txt := base64.StdEncoding.EncodeToString(curBuffer)
		index += remaining

		result, err = vmGACommand(vm, fmt.Sprintf(`{ "execute": "guest-file-write", "arguments": { "handle": %s, "buf-b64": "%s" }}`, guestFileId, base64Txt))

		if err != nil {
			return err
		}
	}

	_, err = vmGACommand(vm, fmt.Sprintf(`{ "execute": "guest-file-close", "arguments": { "handle": %s }}`, guestFileId))

	if err != nil {
		return err
	}	

	return nil
}

func vmCopyFileToHost(vm VirtualMachine, guestPath string, hostPath string) error {
	outCommandPattern := `{ "execute": "guest-file-open", "arguments": { "path": "%s", "mode": "r" }}`

	result, err := vmGACommand(vm, fmt.Sprintf(outCommandPattern, guestPath))

	if err != nil {
		return err
	}
	
	guestFileId := regexFirstGroup(`"return": ([0-9]{1,8})`, result)

	if guestFileId == "" {
		fmt.Printf("%s\n", result)
		return errors.New("Was unable to open file on guest!")
	}


	hostFile, err := os.Create(hostPath)

	if err != nil {
		return err
	}

	for{
		data, err := vmGACommand(vm, fmt.Sprintf(`{"execute": "guest-file-read", "arguments": { "handle": %s, "count": 1024 }}`, guestFileId))

		if err != nil {
			return err
		}

		txt := regexFirstGroup(`"buf-b64": "([A-Za-z0-9=]+)"`, data)
		eof := regexFirstGroup(`"eof": (true|false)`, data)

		buff, err := base64.StdEncoding.DecodeString(txt)

		hostFile.Write(buff)

		if eof == "true" {
			break
		}

	}

	hostFile.Close()

	_, err = vmGACommand(vm, fmt.Sprintf(`{ "execute": "guest-file-close", "arguments": { "handle": %s }}`, guestFileId))

	if err != nil {
		return err
	}	


	return nil
}
