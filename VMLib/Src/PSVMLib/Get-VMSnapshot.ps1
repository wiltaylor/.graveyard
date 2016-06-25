<# 
.SYNOPSIS
	Get snapshots on VM
.DESCRIPTION
	Get all snapshots on target vm.
.EXAMPLE
	Retrives all the snapshots on $vm
	$snapshots = $vm | Get-Snapshot
.OUTPUTS
	IVMSnapshot
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Get-VMSnapshot
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM)

	Process 
	{
		foreach($i in $VM.Snapshots) { $i }
	}
}