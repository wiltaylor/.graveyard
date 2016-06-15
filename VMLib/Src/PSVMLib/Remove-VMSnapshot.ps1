<# 
.SYNOPSIS
	Removes snapshot on VM.
.DESCRIPTION
	Removes the target snapshot
.EXAMPLE
	Remove snapshot stored in $snapshot
	$snapshot | Remove-VMSnapshot
.INPUTS
	IVMSnapshot
#> 
[CmdletBinding]
function Remove-VMSnapshot
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$Snapshot, [string]$Name)

	Process 
	{

	}
}