# SPANR Testing Tool
This is a simple tool designed to assist with testing SPANR resources 
but there is no reason it couldn't be used for other things.

The goal of this tool is to follow the UNIX philosphy for doing one
thing and doing it well.

So this tool can execute tests writen in any language that can write 
to stdout and will generate a report at the end on tests that have
run and which ones have failed.

## Installation
Installation is really simple you just need to grab the executable from
release and run it where it is or copy it into /usr/bin. The
portability of the binary is due to how its used with SPANR. It is 
expected to be copied to a system under configuration with SPANR and
then removed once everything is installed and finished.

There are binaries for Linux, Windows and macOS.

## Building
To build simply run the following:

```bash
$> go get
$> go build
```

There is also a buildall.sh script you can run which will build all
the different supported operating system binaries in the build folder.

## Testing
You can self test spanr-test by running it against the test.yaml file 
in the example folder. This test will report the expected result for
each test.

## Usage
The tool is fairly simple to use, you just need to create a test suite 
yaml file like the following.

```yaml
# You can create a list of test sets which will be run
# Each test set is in the context of one script file.

- name: TestSet1 #Name of test script (must have no spaces)
  description: "This is my awesome test" #Description of your test
  command: python #Executable to run tests with in this set.
  arguments: ['myscript.py', '{TESTNAME}'] #Array of arguments to run
    #on each test.
    # {TESTNAME} is replaced with the name of the test and 
    # {PWD} is replaced with the directory relative to the yaml file.
    # The current directory is also changed to the same folder as the
    # yaml file while its running too.
  tests: ['test1', 'test2', 'test3'] # This is the list of tests in the
    # test suite. The script is executed multiple times with each
    # parameter from this list passed in where {TESTNAME} is in the
    # arguments.
  timeout: 90 # How long in seconds a test can run before it times out.
  continueonerror: yes #Should testing continue on a failed test.

- name: TestSet2
  description: 'My 2nd awesome test set'
  command: ruby
  arguments: ['test.rb', '{TESTNAME}']        
  tests: ['test4', 'myawesometest']
  timeout: 180
  continueonerror: no
```

Once you have a test file you can execute it by calling run.

```bash
$ spanr-test run ./test.yaml
```

You can also list the tests in a .yaml file

```bash
$ spanr-test list ./test.yaml
```

## Return codes
The tool has the following error codes:

* 0 - All completed ok
* 4 - Some tests failed (ignored tests don't count towards this)
* 5 - There was an error in spanr-test

## License 
Copyright 2019 Wil Taylor

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.