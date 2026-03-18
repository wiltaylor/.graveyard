using namespace VMLib.VM;

<# 
.SYNOPSIS
	Creates a new VMInfo Object
.DESCRIPTION
	Creates a new VMInfo Object which is used with New-VM to create a new virtual machine.		
.EXAMPLE
	Create vm info with list of hardware objects
	$info = New-VMInfo
.OUTPUTS
	A IVMwareInfo class with virtual machine details listed in the.
#> 
[CmdletBinding]
function New-VMInfo
{
	param()

	Process 
	{
		New-Object VMCreationInfo
	}
}