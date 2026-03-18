<# 
.SYNOPSIS
	Gets a shared folder on VM
.DESCRIPTION
	Gets a shared folder on the VM.
.PARAMETER Name
	Name of share to return. If left blank then all shares are returned.
.EXAMPLE
	Get all shares from VM
	$shares = $vm | Get-VMShare
.OUTPUTS
	IVMShare
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Get-VMShare
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, [string]$Name)

	Process 
	{

	}
}