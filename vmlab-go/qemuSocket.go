package main

import(
	"net"
	"errors"
	"encoding/json"
)

type QemuVMSocket struct {
	socket net.Conn
	events chan QemuVMEvent
  responses chan QemuVMResponse
	closeSock bool
}

type QemuVMEvent struct {
	name string
	data map[string]interface{}
}

type QemuVMResponse struct {
	error bool
	message string
	value interface{}
}

type QemuCommandBundle struct {
	Execute string `json:"execute"`
	Arguments map[string]interface{}`json:"arguments,omitempty"`
}



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

func (target* QemuVMSocket)processSockRead() {
	buffer := make([]byte, 1024)
	text := ""

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
