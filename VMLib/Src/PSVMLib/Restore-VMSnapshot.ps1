<# 
.SYNOPSIS
	Restores snapshot of VM.
.DESCRIPTION
	Creates a snapshot of target VM.
.EXAMPLE
	Restores snapshot in $snapshot object.
	$snapshot | Restore-VMSnapshot
.INPUTS
	IVMSnapshot
#> 
[CmdletBinding]
function Restore-VMSnapshot
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$Snapshot, [string]$Name)

	Process 
	{

	}
}