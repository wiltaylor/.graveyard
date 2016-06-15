<# 
.SYNOPSIS
	Powers on VM
.DESCRIPTION
	Starts up a Virtual Machine.
.EXAMPLE
	Power up virtual machine.
	$vm | Start-VM
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Start-VM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM)

	Process 
	{

	}
}