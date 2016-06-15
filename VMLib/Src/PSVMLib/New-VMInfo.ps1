<# 
.SYNOPSIS
	Creates a new VMInfo Object
.DESCRIPTION
	Creates a new VMInfo Object which is used with New-VM to create a new virtual machine.		
.PARAMETER Memory
	Ammount of memory in MB to add to the virtual machine.
.PARAMETER CPUs
	Number of CPUs to attach to the virtual machine.
.PARAMETER Cores
	Number of cores each CPU has.
.PARAMETER GuestOS
	GuestOS that virtual machine will be running.
.PARAMETER Hardware
	Array of hardware to add to the virtual machine.
.EXAMPLE
	Create vm info with list of hardware objects
	New-VMInfo -Memory 1024 -CPUs 1 -Cores 2 -GuestOS Windows7x64 -Hardware $hardwarelist
.OUTPUTS
	A IVMwareInfo class with virtual machine details listed in the.
#> 
[CmdletBinding]
function New-VMInfo
{
	param()

	Process 
	{

	}
}