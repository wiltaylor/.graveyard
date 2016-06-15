<# 
.SYNOPSIS
	Connects to a hypervisor.
.DESCRIPTION	This command will connect to a hypervisor ready to interacte with Virtual Machines. You must call this before any other cmdlet.
	
	To get a list of available hypervisors run Get-Hypervisors.
.PARAMETER Name
	Hypervisor to connect to.
.PARAMETER Properties
	Optional hash table to pass through to hypervisor. This is generally only required for remote connections.
.EXAMPLE
	Connect to vmware workstation
	Connect-Hypervisor -Name VMwareWorkstation
#> 
[CmdletBinding]
function Connect-Hypervisor
{
	
	param([parameter(Mandatory = $true)][string]$Name, [HashTable]$Properties)

	Process
	{
		
	}
}