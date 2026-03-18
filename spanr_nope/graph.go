package main

//Configuration State
const (
	CFGNotRun         = iota //Config Item not yet run
	CFGConfigured     = iota //Config Item has been configured
	CFGRebootRequired = iota //Config Item requires a reboot
	CFGNotConfigured  = iota //Config Item not configured
	CFGError          = iota //Config Item error
	CFGSkipOnDep      = iota //Config Item is skipped due to failed condition (this is not a fail)
)

func printCFG(value int) string {
	switch value {
	case CFGNotRun:
		return "NotRun"
	case CFGConfigured:
		return "Configured"
	case CFGRebootRequired:
		return "RebootRequired"
	case CFGNotConfigured:
		return "NotConfigured"
	case CFGError:
		return "ERROR"
	case CFGSkipOnDep:
		return "Skipped Due to Dependancy"
	default:
		return "Unknown"
	}
}

//RunTimeInfo - Holds Runtime information
type RunTimeInfo struct {
	Name string   //Friendly name of runtime
	Path []string //List of paths that are in runtime folder to prepend to path on execution.
}

//ConfigInfo - Holds A configuration script
type ConfigInfo struct {
	Name        string       //Name of configuration script
	Author      string       //Who created the script
	Version     string       //Version of script
	Description string       //Description of script
	Items       []ConfigItem //All the configuration items in script
	Condition   string       //only apply if environment variable is set. Use ! to invert it.
}

//ConfigItem - Holds the definition of a configuration item
type ConfigItem struct {
	Name      string            //Unique name of configuration item, used to identify it.
	Resource  string            //Name of resource this configuration item uses.
	Condition string            //Conditional used to dermine if item is run or not. Use environment variable name or ! to test inverse.
	Options   map[string]string //A hash map of configuration settings passed to resource script
	State     int               //Contains the current state of config item
}

//GatherInfo - Holds info on gatherer
type GatherInfo struct {
	Name        string   //Friendly name of gatherer
	Description string   //Description of gatherer
	Author      string   //Author of gatherer
	Version     string   //Version of gatherer
	Command     string   //Command to be run by gatherer
	Arguments   []string //Arguments to be passed to gatherer
}

//ResourceInfo - Holds info on a resource
type ResourceInfo struct {
	Name           string          //Unique name of resource
	Description    string          //Description of resource
	Author         string          //Author Author of resource
	Version        string          //Version of resource
	TestCommand    string          //Command to run for resource test
	ApplyCommand   string          //Command to run for resource apply
	TestArguments  []string        //Arguments to run when testing resource
	ApplyArguments []string        //Arguments to run when applying resource
	Properties     map[string]bool //Properties resource supports. Boolean specifies if property is mandatory or not
	Path           string          //Set by loader to the directory of the resource files.
}
