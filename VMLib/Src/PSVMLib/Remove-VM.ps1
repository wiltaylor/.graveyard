<# 
.SYNOPSIS
	Removes virtual machine
.DESCRIPTION
	Instructs hypervisor to remove virtual machine. This will result in the virtual machine being deleted.
.EXAMPLE
	Remove the virtual machine
	$myvm | Remove-VM
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Remove-VM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM)

	Process 
	{
		
	}
}