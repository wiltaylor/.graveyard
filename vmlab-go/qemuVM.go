package main

import (
  "path/filepath"
	"errors"
)

type VMSTATUS int

const (
	VMSTATUS_UNPROVISIONED VMSTATUS = iota
	VMSTATUS_OFF
	VMSTATUS_RUNNING
	VMSTATUS_PAUSED
	VMSTATUS_ERROR
	VMSTATUS_UNKNOWN
)

type QemuVM struct {
	SocketConnected bool
  ControlSocket QemuVMSocket
	GuestAgentSocket QemuVMSocket
	GraphObj VirtualMachine
	TemplateObj TemplateFile
	HDPath string
	VMPath string
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
