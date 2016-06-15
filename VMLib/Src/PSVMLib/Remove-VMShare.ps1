<# 
.SYNOPSIS
	Removes a shared folder on VM
.DESCRIPTION
	Removes a shared folder on the VM.
.PARAMETER Name
	Name of share.
.EXAMPLE
	Remove install share from vm.
	$vm | Remove-VMShare -Name Installs
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Remove-VMShare
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM)

	Process 
	{

	}
}