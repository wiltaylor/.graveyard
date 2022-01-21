package main

import (
  "gopkg.in/yaml.v2"
  "io/ioutil"
)

type vmlabFile struct {
  Title string `yaml:'title'`
  Description string `yaml:'description'`
  Author string `yaml:'author'`
  VirtualMachines []VirtualMachine `yaml:'virtualmachines'`
}

type VirtualMachine struct {
  Name string `yaml:'name'`
  Template string `yaml:'template'`
  Cpus int `yaml:'cpus'`
  Memory int `yaml:'memory'`
}

func loadLabFile(path string) (vmlabFile, error) {
  var result vmlabFile
  data, err := ioutil.ReadFile(path)

  if err != nil {
    return result, err
  }

  err = yaml.Unmarshal(data, &result)
  return result, err
}

func writeLabFile(path string, data vmlabFile) error {
  byteData, err := yaml.Marshal(&data)
  
  if err != nil {
    return err
  }

  err = ioutil.WriteFile(path, byteData, 0755)

  return err
}

