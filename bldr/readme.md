# BLDR - Simple build orchestration tool
BLDR is a build tool that uses containers to isolate build tool chains. This
allows for the same tool chain to be used on developer machines and Continuous
integration servers.

## Building
There are 2 ways to build the project.

BLDR way:
```bash
$ bldr build
$ bldr package
```

Manual way (if you don't have bldr installed):
```bash
$ cargo build --release
```

Once you have the above built the executable will be located in target/release
as a single binary (bldr or bldr.exe depending on the platform).

## Installing
Simply put bldr executable in your path.

## Using
To use bldr you first must createa  bldr folder in your project and a bldr.yaml
file in the root of your project.


### Command line
Command line options are as follows:

```bash
$ bldr [action] 
```

bldr has the following actions built in:

* init - Instructs docker to create/recreate all of the build chain images
  stored in the local bldr folder.
  
* destroy - Instructs docker to remove all of the build chain images specified in
  the bldr folder.
  
* ls - Lists all the available actions in the bldr.yaml file.

On top of the built in actions you can also specify actions by name specified in
the bldr.yaml file and they will run. All additional arguments you pass to your
custom actions will be passed at the end of the command line.

### blrd folder
In the bldr folder you need to create docker files for the tool chains you want
to create. Create them with the naming format of **Dockerfile-toolchainname**
where you replace toolchain with the name you want to reference the toolchain
as.

### bldr.yaml file
The bldr.yaml file is where you define the actions you want to be able to
execute to build your software. Here is an example file which is used to build
bldr itself.

```yaml
name: bldr
folders: 
        - host_path: .
          virt_path: /prj
actions:
        - name: build
          act_type: Meta
          depend: ["build_linux_x64", "build_windows_x64", "build_macos_x64"]

        - name: build_linux_x64
          description: Build for linux
          act_type: OneShot
          image: bldr:rust
          command: "cargo"
          args: ["build", "--release", "--target", "x86_64-unknown-linux-gnu"]
          working_dir: "/prj"

        - name: build_windows_x64
          description: Build for windows
          act_type: OneShot
          image: bldr:rust
          command: "cargo"
          args: ["build", "--release", "--target", "x86_64-pc-windows-gnu"]
          working_dir: "/prj"

        - name: build_macos_x64
          description: Build for macOS
          act_type: OneShot
          image: bldr:rust
          command: "cargo"
          args: ["build", "--release", "--target", "x86_64-apple-darwin"]
          working_dir: "/prj"

        - name: package
          act_type: Meta
          depend: [ "package_linux_x64", "package_windows_x64", "package_macos_x64"]

        - name: package_linux_x64
          description: Package Linux binary
          act_type: OneShot
          image: bldr:rust
          command: tar
          args: ["-czvf", "bldr-linux.tar.gz", "./target/x86_64-unknown-linux-gnu/release/bldr"]
          working_dir: "/prj"

        - name: package_windows_x64
          description: Package Windows binary
          act_type: OneShot
          image: bldr:rust
          command: "zip"
          args: ["-9", "bldr-windows.zip", "./target/x86_64-pc-windows-gnu/release/bldr.exe"]
          working_dir: "/prj"

        - name: package_macos_x64
          description: Package macOS binary
          act_type: OneShot
          image: bldr:rust
          command: "zip"
          args: ["-9", "bldr-macos.zip", "./target/x86_64-apple-darwin/release/bldr"]
          working_dir: "/prj"

        - name: debug
          description: Version of virtual environment
          act_type: OneShot
          image: bldr:rust
          command: "zip"
          args: ["--help"]
```

#### Top Level sections
The top level sections are as follows:

* name - Name of the project. Must not contain spaces. This is used to name the
  docker container with all the tool chains being tags of this container.
  
* (Optional)description - A description of the project.

* (Optional)folders - You can specify folders that are shared into all tool chains
  globally in here.
  
* actions - Actions are where you specify which commands to run and which
  tool chain container to run them in as part of your build.
  
#### Folders
Folders lets you specify folders to be shared with the docker container as
volumes as part of the build. There are useful in letting the tool chain access
your source code and save out artefacts.

Each folder has the following properties:

* host_path - Path on the host system to be shared (this can be relative to the
  current directory).
  
* virt_path - Path in the container for the folder to be mounted.

* (Optional)no_fix - Set to true if you don't want bldr to chown the contents of the
  folder as the running user after its completed the build action. The chown
  step is run to resolve issues where containers have a different uid to the
  user logged into the host.
  
#### Actions
Actions are where you can specify the commands you want to run to build your
software and what tool chain container to run them in.

Each action has the following properties:

* name - Name of the action. This is used to call the action from the
  command line and also is the tag part of the container name.
  
* (optional) description - Description of the action being performed. This will
  show up when a user runs bldr ls.
  
* act_type - This is the action to be performed. It can be one of the following:
    - OneShot - Will run the command in a container and dispose of the container
      afterwards.
    - Persist - Will run the command in a container and detach from it.
    - Host - will run the command on the host outside of any container.
    - Kill - Will terminate a previously started container with Persist.
    - Meta - Doesn't do anything. This is used to have an action that runs
      dependencies but doesn't do anything itself.
      
* (optional)image - This is the docker image to run the command in. This is normally in
  *projectname:toolchain* name format. However you can also put any other
  package that is in dockerhub (or any other registry if its setup with docker).
  This is ignored if you have act_type set to Host.
  
* (optional)net - This lets you specify the network you want to attach to the container.
  e.g use host if you want to attach the docker container locally to the hosts
  nic. Useful for hosting servers for testing.
  
* (optional)folders - This lets set folders to share into the container on a per
  action basis. Instead of globally like the project level entry.
  
* (optional)depend - This is a list where you can specify which actions will run before
  this one as part of the build.
  
* (optional)command - This lets you specify the executable to run in the docker
  container. Do not specify arguments here, use args instead.
  
* (optional)args - This lets you specify a list of arguments to pass to the
  command that is executed in the toolchain container.
  
* (optional)working_dir - This lets you set the working directory inside the
  tool chain container to use while running the command.


## Contributing
Create an issue for the item you want to fix/add and then create a pull request
referencing it.

## License 
Copyright 2020 Wil Taylor (bldr@wiltaylor.dev)

Permission is hereby granted, free of charge, to any person obtaining a copy of 
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR 
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS 
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR 
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER 
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN 
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
