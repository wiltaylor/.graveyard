<# 
.SYNOPSIS
	Powers off VM
.DESCRIPTION
	Shutsdown up a Virtual Machine.
.PARAMETER Force
	Force virtual machine to stop. If not set hypervisor will ask guest operating system to shutdown.
.EXAMPLE
	Power off virtual machine.
	$vm | Stop-VM
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Stop-VM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, [switch]$Force)

	Process 
	{

	}
}