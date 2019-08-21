# SPANR Configuration Tool
SPANR is a simple configuration managment tool which follows the unix philosphy of
do one thing and do it well.

As a result SPANR lets you write your resources in whatever native language is
on the platform you want to run it on. i.e. Bash, Python or PowerShell or anything
else that can write to STDOut.

The other focus of SPANR is it's portability, you can drop the binary onto the 
target system and just run it, you don't need to install it.


## Installation
As this is a portable tool you don't need to install it, just copy the binaries on 
to the target system and run them.

## Building
To build simply run the following:

```bash
$> go get
$> go build
```

There is also a buildall.sh script you can run which will build all
the different supported operating system binaries in the build folder.

## Testing
There is also a tool for testing resources located in [this](https://github.com/wiltaylor/spanr-test) repository.

## Usage
### Command line

Creating a new configuration folder
```bash
$ spanr init /path/to/folder
```

How to apply configuration in a config folder
```bash
$ spanr run /path/to/config/folder
```

How to apply configuration folder with a properties file.
```bash
$ spanr run /path/to/config/folder -p /path/to/properties.yaml
```

How to only test configuration and not apply anything
```bash
$ spanr run /path/to/config/folder -t
```

How to output results into a yaml file.
```bash
$ spanr run /path/to/config/folder -o /path/to/outputfile.yaml
```

How to list all the resources, gathers and configuration info

```bash
$ spanr ls /path/to/config/folder
```

### Return codes
The tool has the following error codes:

* 0 - Configuration returned all configured.
* 5 - There was an error or configuration wasn't applied.
* 3010 - Configuration requires a reboot before it can continue.

## More details on components

### Configuration folder
Before you can use the tool you need to create a configuration folder. A configuration folder contains everything
you need to apply a configuration to a system.

A configuration folder contains the following:

* A config.yaml file - This contains a list of the configurations you want applied to the system and the settings being applied.
* A runtimes.yaml file and a runtimes folder. This allows you to add things to the start of the path while the tool is being run (e.g. a portable version of python or ruby).
* resources folder - This folder contains resources which are scripts used to apply configuration to the system. See the resource section below.
* gatherer - This folder contains gatherers which are scripts that collect information from the system and make them available to your resources 
and config.yaml script.

To create a configuration folder it is simply a matter of running the following command.

```bash
$ spanr init /path/to/folder
```

This will create a skeleton for you to configure.

### Config file
The config.yaml file contains all of the settings you want to apply
to the target system.

Example:
```yaml
name: "Name of config here"
author: "Your name here"
description: "Your script description here"
version: "0.1.0"
items: 
  - name: "MyConfig"
    resource: "MyResource"
    options:
      op1: "5" 
      op2: "123"
```

The top of the file is mostly just metadata to give you an idea
of what its purpose is. The items section is where you define
all the configuration you want to do.

* name - a unique name of the configuration item
* resource - the resource you want to use for this configuration item.
* options - Here you can create a list of options to give to the resource.

### Resources
You can create resources by creating a folder under resources for your resource and placing a resource.yaml file inside it.
Inside the yaml file you can specify any number of "resources" which can then be used by a configuration to make changes to
the system.

An example of a resource is as follows:

```yaml
- name: "MyResource"
  description: "my test resource"
  author: "Wil Taylor"
  version: "0.1.0"
  testcommand: "python"
  applycommand: "python"
  testarguments: ["resource.py", "test"]
  applyarguments: ["resource.py", "apply"]
  properties:
    testprop1: no
    testprop2: yes
```
First thing you will notice is this is a list, which means you can define as many as you want in a single file.

Next up you have a bunch of metadata like name, description etc. The name is important as this is how you
reference this resource in your config.yaml file.

You will also see testcommand, testarguments, applycommand and applyarguments. These are the scripts which are 
called to execute the actions. For every resource you must have a script to test if its configured and one to 
apply the configuration when its not.

The logic is SPANR will run all of the test commands to check if a configuration is set. If it is already set 
it will continue on to the next item in the configuration and if its not it will run the apply action. Once the 
apply action has completed it will run the test action again to confirm it actually was set (unless you return
must reboot from script in which case it will exit with 3010).

Finally you can list the properties that this resource accepts.

### Writing the scripts
Creation of the scripts has been made as simple as possible. All properties set for the resource or captured
by gatherers will be set as environment variables for the process running the script (not the global system).

This means you just have to check environment variables in your language of choice.

Next to send responses back to SPANR you can write special strings to STDOUT.

* \#\#FAIL\#\# - The script fails for some reason. If you don't write anything this is what the results defaults to.

* \#\#CONFIGURED\#\# - If passed during a test it indicates it is already configured and if sent in apply it indicates it succsefully applied configuration.

* \#\#REBOOT\#\# - Configuration was succesful but the system needs to be rebooted. This is mainly used on windows.

* \#\#NOTCONFIGURED\#\# - Returned when running test and config item
is not currently configured.

* \#\#SPANR\[name=value\]\#\# - Sets an environment variable
for all scripts run from this SPANR session (not global system).

* \#\#SPANRMSG\[message\]\#\# - Prints a message to the stdout of
SPANR.

### Gatherers 
A gatherer is similar to a resource except rather than making changes
to the system they gather information from the system. The idea is 
these scripts gather information about the current system which
help resources make decisions about what to configure on the system
and also allow config.yaml files to be paramatised.

Some examples of what you could create resources for are as follows:

* Operating System you are running on (windows, linux mac etc)

* Serial number of the system

The idea is you should use gatherers to find out information about a 
system and not put it into your resources to help seperate their 
scope.

To create a gatherer create a folder inside the gatherers folder and
create a gather.yaml inside it.

Example file:

```yaml
name: "TestGather1"
description: "Example test gatherer"
author: "Wil Taylor"
version: "0.1.0"
command: "python"
arguments: ['test.py']
```

As you can see it basically just contains a bunch of metadata and
details on a script to run. You also can't chain a heap of different
gatherers in a single file. This is because you don't get to select 
which gatherers run, it runs all of them.

To create a gatherer script all you need to do is create a script in
any language you want which spits out `##SPANR[name=value]##` to 
stdout.

With all the properties you want to have sent to other SPANR scripts.

## License 
Copyright 2019 Wil Taylor

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.