<# 
.SYNOPSIS
	Connects to a hypervisor.
.DESCRIPTION
	This command will connect to a hypervisor ready to interacte with Virtual Machines. You must call this before any other cmdlet.
	
	To get a list of available hypervisors run Get-Hypervisors.
.PARAMETER Name
	Hypervisor to connect to.
.PARAMETER Settings
	Info object passed in to detail connection details for cmdlet. Use Net-HypervisorInfo to create this object.
.EXAMPLE
	Connect to vmware workstation
	Connect-Hypervisor -Name VMwareWorkstation
#> 
[CmdletBinding]
function Connect-Hypervisor
{
	
	param([parameter(Mandatory = $true)][string]$Name, [parameter(Mandatory = $true)]$Settings)

	Process
	{
		$script:HypervisorFactory.CreateHypervisor($Name, $Settings)
	}
}