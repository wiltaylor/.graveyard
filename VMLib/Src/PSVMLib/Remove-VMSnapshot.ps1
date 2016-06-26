<# 
.SYNOPSIS
	Removes snapshot on VM.
.DESCRIPTION
	Removes the target snapshot
.EXAMPLE
	Remove snapshot name mysnapshot
	$VM | Remove-VMSnapshot -Name mysnapshot
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Remove-VMSnapshot
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, [string]$Name)

	Process 
	{
		$VM.RemoveSnapshot($Name)
	}
}