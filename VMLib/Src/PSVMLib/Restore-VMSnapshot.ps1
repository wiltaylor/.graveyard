<# 
.SYNOPSIS
	Restores snapshot of VM.
.DESCRIPTION
	Creates a snapshot of target VM.
.PARAMETER Name
	Name of snapshot to restore.
.EXAMPLE
	Restores snapshot in $snapshot object.
	$VM | Restore-VMSnapshot -Name MySnapshot
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Restore-VMSnapshot
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, [string]$Name)

	Process 
	{
		$VM.RestoreSnapshot($Name)
	}
}