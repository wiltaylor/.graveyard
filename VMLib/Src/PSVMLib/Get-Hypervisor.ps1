<# 
.SYNOPSIS
	Returns a list of available hypervisors
.DESCRIPTION
	Returns the names of all the hypervisors available to connect to on the system.

	Please use Connect-Hypervisor to connect to hypervisor.
.EXAMPLE
	Get a list of all available hypervisors to connect to.
	Get-Hypervisor
.OUTPUTS
	String array containing all the hypervisor names.
#> 
[CmdletBinding]
function Get-Hypervisor
{
	param()
	
	Process
	{

	}
}