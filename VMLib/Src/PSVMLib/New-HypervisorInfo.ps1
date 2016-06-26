<# 
.SYNOPSIS
	Creates a hypervisor info object.
.DESCRIPTION
	This command will create a hypervisor info object which is required to connect to a hypervisor.
.PARAMETER Name
	Hypervisor type to create object for. Use Get-Hypervisors to get names of supported hypervisors.
.EXAMPLE
	Retrives a hypervisor info object used to create a new Hypervisor instance.
	New-HypervisorInfo -Name VMwareWorkstation
#> 
[CmdletBinding]
function New-HypervisorInfo
{
	param([parameter(Mandatory=$true)][string]$Name)

	$script:HypervisorFactory.CreateHypervisorConnectionInfo($Name)

}