<# 
.SYNOPSIS
	Restart off VM
.DESCRIPTION
	Restart a Virtual Machine.
.PARAMETER Force
	Force virtual machine to restart. If not set hypervisor will ask guest operating system to restart.
.EXAMPLE
	Restart virtual machine.
	$vm | Restart-VM
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Restart-VM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, [switch]$Force)

	Process 
	{

	}
}