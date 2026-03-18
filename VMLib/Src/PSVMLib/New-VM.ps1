<# 
.SYNOPSIS
	Creates a new VM.
.DESCRIPTION
	Uses the currently connected hypervisor to create a new virtual machine.
.PARAMETER Name
	Name of new virtual machine to create.
.PARAMETER Path
	Path to create new virtual machine at.
.PARAMETER Info
	Info object to use to create virtual machine. See New-VMInfo for more information on 
	how to create one fo these.
.PARAMETER Hypervisor
	Hypervisor to create the vm on.
.EXAMPLE
	Create a new virtual machine
	New-VM -Name MyVM -Path c:\VMs\Myvm.vmx -Info $MyVMinfoObject
.INPUTS 
	A hypervisor object.
.OUTPUTS
	A IVirtualMachine object.
#> 
[CmdletBinding]
function New-VM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$Hypervisor, [Parameter(Mandatory=$true)][string]$Name, [Parameter(Mandatory=$true)][string]$Path,[Parameter(Mandatory=$true)]$Info)

	Process 
	{
		$Hypervisor.CreateNewVM($Info)
	}
}